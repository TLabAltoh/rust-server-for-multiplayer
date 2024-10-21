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
    window.open(document.URL + (document.duplicated ? "" : "?room_id=" + String(room_id)), '_blank');
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

function onTrack(event) {
    const track = event.track;
    const stream = event.streams[0];

    var mediaReceive = null;
    switch (track.kind) {
        case "video":
            mediaReceive = document.getElementById("video_recv");
            break;
        case "audio":
            mediaReceive = document.getElementById("audio_recv");
            break;
    }

    if (mediaReceive.srcObject !== stream) {
        mediaReceive.srcObject = stream;
        console.log('received remote stream', event);
    }
}

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
                        const stream = document.audioStream ? document.audioStream : document.videoStream;
                        peerConnection.whip(json, stream);
                        break;
                    case "stream/whep":
                        peerConnection.whep(json, onTrack.bind(this));
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

    const queries = getUrlQueries();
    document.duplicated = false;
    if (queries["room_id"]) {
        document.duplicated = true;
        var room_id = Number(queries["room_id"]);

        ["room", "room/join", "room/exit", "room/delete", "stream/whip", "stream/whep", "ws/connect"].forEach((elem_id) => {
            document.getElementById(elem_id).getElementsByName("room_id")[0].setAttribute("value", room_id);
        });
    }

    const video = document.getElementById("video");

    document.videoRecordingFlg = false;
    document.videoStream = null;

    video.addEventListener("play", startVideoRecording);
    video.addEventListener("stop", endVideoRecording);

    document.audioRecordingFlg = false;
    document.audioStream = null;
    document.audioProcessor = null;
    document.audioAnalyser = null;

    const audioSend = document.getElementById("audio_send");
    audioSend.addEventListener("play", () => {
        startAudioRecording(true);
    });
    audioSend.addEventListener("stop", endAudioRecording);

    const audioRecv = document.getElementById("audio_recv");
    audioRecv.addEventListener("play", () => {
        startAudioRecording(false);
    });
    audioRecv.addEventListener("stop", endAudioRecording);

    document.canvas = document.getElementById("whip_audio_canvas");
    document.canvasContext = document.canvas.getContext("2d");
});

function startVideoRecording() {
    document.videoRecordingFlg = true;

    const video = document.getElementById("video");

    let stream;
    const fps = 0;
    if (video.captureStream) {
        stream = video.captureStream(fps);
    } else if (video.mozCaptureStream) {
        stream = video.mozCaptureStream(fps);
    } else {
        console.error('Stream capture is not supported');
        stream = null;
    }

    document.videoStream = stream;
}

function endVideoRecording() {
    document.videoRecordingFlg = false;

    const stream = document.videoStream;
    const tracks = stream.getTracks();

    tracks.forEach((track) => {
        track.stop();
    });

    document.videoStream = null;
};

function onAudioProcess(e) {
    if (!document.audioRecordingFlg) return;

    var input = e.inputBuffer.getChannelData(0);

    var bufferData = new Float32Array(document.bufferSize);
    for (var i = 0; i < document.bufferSize; i++) {
        bufferData[i] = input[i];
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

function startAudioRecording(isWhip) {
    document.audioRecordingFlg = true;

    document.bufferSize = 1024;
    document.audioData = [];

    document.isWhip = isWhip;
    var audioId;
    var canvasId;
    if (isWhip) {
        audioId = "audio_send";
        canvasId = "whip_audio_canvas";
    } else {
        audioId = "audio_recv";
        canvasId = "whep_audio_canvas";
    }

    const audio = document.getElementById(audioId);

    document.canvas = document.getElementById(canvasId);
    document.canvasContext = document.canvas.getContext("2d");

    document.audioContext = new AudioContext();
    document.isFirefox = false;

    if (audio.captureStream) {
        document.audioStream = audio.captureStream();
    } else if (audio.mozCaptureStream) {
        document.audioStream = audio.mozCaptureStream();
        document.isFirefox = true;
    } else {
        console.error('Stream capture is not supported');
        document.audioStream = null;
    }

    document.audioProcessor = document.audioContext.createScriptProcessor(document.bufferSize, 1, 1);
    var mediaStreamSource = isWhip ? new MediaElementAudioSourceNode(document.audioContext, { mediaElement: audio }) : document.audioContext.createMediaStreamSource(document.audioStream);
    mediaStreamSource.connect(document.audioProcessor);
    document.audioProcessor.onaudioprocess = onAudioProcess;
    document.audioProcessor.connect(document.audioContext.destination);

    document.audioAnalyser = document.audioContext.createAnalyser();
    mediaStreamSource.connect(document.audioAnalyser);
    document.audioAnalyser.fftSize = 2048;
};

function endAudioRecording() {
    document.audioRecordingFlg = false;

    const stream = document.audioStream;
    const tracks = stream.getTracks();

    tracks.forEach((track) => {
        track.stop();
    });

    document.audioStream = null;
};