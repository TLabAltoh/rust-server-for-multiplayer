//import { SfuPeerConnection } from "./webrtc-sfu.js";
//import { SfuWebSocket } from "./ws-sfu.js";
//import { SfuClient } from "./sfu-client.js";

var peerConnection = new SfuPeerConnection();
var websocket = new SfuWebSocket();

// https://stackoverflow.com/questions/13905435/javascript-getting-specific-element-of-parent-by-name
Element.prototype.getElementsByName = function (arg) {
    var returnList = [];
    (function BuildReturn(startPoint) {
        for (var child in startPoint) {
            if (startPoint[child].nodeType != 1) continue; //not an element
            if (startPoint[child].getAttribute("name") == arg) returnList.push(startPoint[child]);
            if (startPoint[child].childNodes.length > 0) {
                BuildReturn(startPoint[child].childNodes);
            }
        }
    })(this.childNodes);
    return returnList;
};

function getUrlQueries() {
    var queryStr = window.location.search.slice(1);
    queries = {};

    if (!queryStr) {
        return queries;
    }

    queryStr.split('&').forEach(function (queryStr) {
        var queryArr = queryStr.split('=');
        queries[queryArr[0]] = queryArr[1];
    });

    return queries;
}

function createClient() {
    const room_id = document.getElementById("room/join").getElementsByName("room_id")[0].getAttribute("value");
    window.open(document.URL + "?room_id=" + String(room_id), null);
}

async function post(action) {
    try {
        var response = await window.fetch(action, {
            method: "POST",
        });

        response = await response.text();

        console.log(response);

    } catch (e) {
        console.log(e);
    }
}

const forms = document.getElementsByTagName("form");

const form_action = ["room", "room/join", "room/exit", "room/create", "room/delete", "stream/whip", "stream/whep", "stream/reforward", "stream/infos", "send_rtc_message", "ws/connect", "send_ws_message"];

window.addEventListener('DOMContentLoaded', () => {
    for (var i = 0; i < forms.length; i++) {
        if (form_action.includes(forms[i].getAttribute("action"))) {
            forms[i].addEventListener("submit", async event => {
                event.preventDefault();

                const form = event.currentTarget;
                const action = form.getAttribute("action");

                const formData = new FormData(form);
                const json = {};
                formData.forEach((value, key) => {
                    elem = form.getElementsByName(key)[0];
                    switch (elem.getAttribute("type")) {
                        case "number":
                            json[key] = Number(value);
                            break;
                        case "radio":
                            json[key] = value == "true" ? true : false;
                            break;
                        default:
                            json[key] = value;
                            break;
                    }
                });

                switch (action) {
                    case "send_rtc_message":
                        peerConnection.send(json.sender_message, json.to);
                        break;
                    case "stream/whip":
                        peerConnection.whip(json);
                        break;
                    case "stream/whep":
                        peerConnection.whep(json);
                        break;
                    case "send_ws_message":
                        websocket.send(json.sender_message, json.to);
                        break;
                    case "ws/connect":
                        websocket.join(json);
                        break;
                    default:
                        const jsonStr = JSON.stringify(json);
                        const jsonBase64 = btoa(jsonStr);

                        // console.log("jsonStr: " + jsonStr);
                        // console.log("jsonBase64: " + jsonBase64);

                        try {
                            var response = window.fetch(action + "/" + jsonBase64 + "/", {
                                method: "POST",
                            });

                            response = await response
                                .then(r => {
                                    return r.clone().json()
                                        .then(r => {
                                            r.isJson = true;
                                            return r;
                                        })
                                        .catch(() => {
                                            r = r.text();
                                            r.isJson = false;
                                            return r;
                                        });
                                });

                            switch (action) {
                                case "room":
                                    break;
                                case "room/join":
                                    if (response.isJson) {
                                        ["room/exit", "stream/whip", "stream/whep", "ws/connect"].forEach((elem_id) => {
                                            document.getElementById(elem_id).getElementsByName("user_id")[0].setAttribute("value", response.user_id);
                                            document.getElementById(elem_id).getElementsByName("user_token")[0].setAttribute("value", response.user_token);
                                        });

                                        ["send_ws_message", "send_rtc_message_publish", "send_rtc_message_subscribe"].forEach((elem_id) => {
                                            document.getElementById(elem_id).getElementsByName("to")[0].setAttribute("value", response.user_id);
                                        });
                                    }
                                    break;
                                case "room/exit":
                                    break;
                                case "room/create":
                                    if (response.isJson) {
                                        ["room", "room/join", "room/exit", "room/delete", "stream/whip", "stream/whep", "ws/connect"].forEach((elem_id) => {
                                            document.getElementById(elem_id).getElementsByName("room_id")[0].setAttribute("value", response.room_id);
                                        });
                                    }
                                    break;
                                case "room/delete":
                                    break;
                            }

                            console.log((response.isJson ? "json:" : "text:") + JSON.stringify(response));

                        } catch (e) {
                            console.log(e);
                        }
                        break;
                }
            });
        }
    };

    const videoL = document.getElementById('videoL');
    const videoR = document.getElementById('videoR');

    videoL.addEventListener('canplay', () => {
        let stream;
        const fps = 0;
        if (videoL.captureStream) {
            stream = videoL.captureStream(fps);
        } else if (videoL.mozCaptureStream) {
            stream = videoL.mozCaptureStream(fps);
        } else {
            console.error('Stream capture is not supported');
            stream = null;
        }
        videoR.srcObject = stream;
    });

    var queries = getUrlQueries();
    if (queries["room_id"]) {
        var room_id = Number(queries["room_id"]);

        ["room", "room/join", "room/exit", "room/delete", "stream/whip", "stream/whep", "ws/connect"].forEach((elem_id) => {
            document.getElementById(elem_id).getElementsByName("room_id")[0].setAttribute("value", room_id);
        });
    }
});

window.onload = function () {
    document.mediaStream = null;
    document.scriptProcessor = null;
    document.bufferSize = 1024;
    document.audioData = [];
    document.recordingFlg = false;

    document.audioElement = document.getElementById("audio_source");

    document.audioElement.addEventListener("play", startRecording);
    document.audioElement.addEventListener("stop", endRecording);

    document.canvas = document.getElementById("whip_audio_canvas");
    document.canvasContext = document.canvas.getContext("2d");

    document.audioAnalyser = null;
}

function onAudioProcess(e) {
    if (!document.recordingFlg) return;

    var input = e.inputBuffer.getChannelData(0);
    var output = e.outputBuffer.getChannelData(0);

    var power = document.isFirefox ? 1 : 0;

    var bufferData = new Float32Array(document.bufferSize);
    for (var i = 0; i < document.bufferSize; i++) {
        bufferData[i] = input[i];
        output[i] = input[i] * power;
    }
    document.audioData.push(bufferData);

    analyseVoice();
};

function analyseVoice() {
    var fsDivN = document.audioContext.sampleRate / document.audioAnalyser.fftSize;
    var spectrums = new Uint8Array(document.audioAnalyser.frequencyBinCount);
    document.audioAnalyser.getByteFrequencyData(spectrums);
    document.canvasContext.clearRect(0, 0, document.canvas.width, document.canvas.height);

    document.canvasContext.beginPath();
    document.canvasContext.strokeStyle = "rgb(0 255 0 / 50%)";

    for (var i = 0, len = spectrums.length; i < len; i++) {
        var x = (i / len) * document.canvas.width;
        var y = (1 - (spectrums[i] / 255)) * document.canvas.height;

        if (i === 0) {
            document.canvasContext.moveTo(x, y);
        } else {
            document.canvasContext.lineTo(x, y);
        }
        var f = Math.floor(i * fsDivN);

        if ((f % 1000) === 0) {
            var text = (f < 1000) ? (f + ' Hz') : ((f / 1000) + ' kHz');
            document.canvasContext.fillText(text, x, document.canvas.height);
        }
    }

    document.canvasContext.stroke();

    var baseYs = [1.00, 0.50, 0.00];
    for (var i = 0, len = baseYs.length; i < len; i++) {
        var base = baseYs[i];
        var gy = (1 - base) * document.canvas.height;
        document.canvasContext.fillRect(0, gy, document.canvas.width, 1);
    }
}

function startRecording() {
    document.recordingFlg = true;

    document.audioContext = new AudioContext();
    document.isFirefox = false;

    if (document.audioElement.captureStream) {
        document.mediaStream = document.audioElement.captureStream();
    } else if (document.audioElement.mozCaptureStream) {
        document.mediaStream = document.audioElement.mozCaptureStream();
        document.isFirefox = true;
    } else {
        console.error('Stream capture is not supported');
        document.mediaStream = null;
    }

    document.scriptProcessor = document.audioContext.createScriptProcessor(document.bufferSize, 1, 1);
    var mediastreamsource = document.audioContext.createMediaStreamSource(document.mediaStream);
    mediastreamsource.connect(document.scriptProcessor);
    document.scriptProcessor.onaudioprocess = onAudioProcess;
    document.scriptProcessor.connect(document.audioContext.destination);

    document.audioAnalyser = document.audioContext.createAnalyser();
    document.audioAnalyser.fftSize = 2048;
    mediastreamsource.connect(document.audioAnalyser);
};

function endRecording() {
    document.recordingFlg = false;

    const stream = document.mediaStream;
    const tracks = stream.getTracks();

    tracks.forEach((track) => {
        track.stop();
    });

    document.mediaStream = null;
};