class SfuPeerConnection extends SfuClient {
  constructor() {
    super();
    this.peerConnection;
    this.dataChannel;
  }

  buffer_to_string(buf) {
    return String.fromCharCode.apply("", new Uint8Array(buf));
  }

  whip(json, stream) {
    this.json = json;
    this.action = "stream/whip";

    const servers = {
      iceServers: [{
        urls: "stun:stun.l.google.com:19302"
      }]
    }
    this.peerConnection = new RTCPeerConnection(servers);
    console.log('Created peer connection object');

    this.dataChannel = this.peerConnection.createDataChannel('demoDataChannel');
    this.dataChannel.onmessage = (event) => {
      console.log("message received: ", this.buffer_to_string(event.data));
    };
    console.log('Created send data channel');

    this.peerConnection.onicecandidate = e => {
      this.onIceCandidate(this.peerConnection, e);
    };
    this.dataChannel.onopen = this.ondataChannelStateChange.bind(this);
    this.dataChannel.onclose = this.ondataChannelStateChange.bind(this);

    this.localCandidates = [];
    this.remoteCandidates = [];

    if (stream !== null) {
      stream.getTracks().forEach(track => this.peerConnection.addTrack(track, stream));
    }

    this.peerConnection.createOffer().then(
      this.onCreateOffer.bind(this),
      this.onCreateSessionDescriptionError.bind(this)
    );
  }

  whep(json, ontrack) {
    this.json = json;
    this.action = "stream/whep";

    const servers = {
      iceServers: [{
        urls: "stun:stun.l.google.com:19302"
      }]
    }
    this.peerConnection = new RTCPeerConnection(servers);
    console.log('Created peer connection object');

    this.dataChannel = this.peerConnection.createDataChannel('demoDataChannel');
    this.dataChannel.onmessage = (event) => {
      console.log("message received: ", this.buffer_to_string(event.data));
    };
    console.log('Created send data channel');

    this.peerConnection.onicecandidate = e => {
      this.onIceCandidate(this.peerConnection, e);
    };
    this.dataChannel.onopen = this.ondataChannelStateChange.bind(this);
    this.dataChannel.onclose = this.ondataChannelStateChange.bind(this);

    this.localCandidates = [];
    this.remoteCandidates = [];

    this.peerConnection.ontrack = ontrack;

    this.peerConnection.createOffer(ontrack !== null ? { offerToReceiveAudio: 1 } : null).then(
      this.onCreateOffer.bind(this),
      this.onCreateSessionDescriptionError.bind(this)
    );
  }

  onCreateSessionDescriptionError(error) {
    console.log('Failed to create session description: ' + error.toString());
  }

  gotRemoteStream(event) {
    if (rightVideo.srcObject !== event.streams[0]) {
      rightVideo.srcObject = event.streams[0];
      console.log('pc2 received remote stream', event);
    }
  }

  send(message, to) {
    let byteHedder = this.i32ToUint8Array(to);
    switch (typeof (message)) {
      case "string":
        let byteMessage = new TextEncoder().encode(message);
        this.dataChannel.send(new Uint8Array([...byteHedder, ...byteMessage]));
        console.log("[ws-sfu] send string");
        break;
      case "object":
        byteMessage = message;
        this.dataChannel.send(new Uint8Array([...byteHedder, ...byteMessage]));
        break;
    }
    console.log('Send Message: ' + message);
  }

  closeDataChannels() {
    console.log('Closing data channels');
    this.dataChannel.close();
    console.log('Closed data channel with label: ' + this.dataChannel.label);
    this.peerConnection.close();
    this.peerConnection = null;
    console.log('Closed peer connections');
  }

  async onCreateOffer(desc) {
    this.peerConnection.setLocalDescription(desc);
    console.log(`On create offer:\n${desc.sdp}`);

    this.json.offer = desc.sdp;

    const jsonStr = JSON.stringify(this.json);
    const jsonBase64 = btoa(jsonStr);

    console.log("json:" + jsonStr);

    const sock = new WebSocket("ws://localhost:7777/" + this.action + "/" + jsonBase64 + "/");
    this.sock = sock;

    sock.addEventListener("open", e => {
      console.log("[ws-rtc] open: " + e);
    });

    sock.addEventListener("message", e => {
      console.log("[ws-rtc] message: " + e);
      var json = JSON.parse(e.data);
      if (!json.is_candidate) {
        desc = new RTCSessionDescription({ type: 'answer', sdp: json.sdp });
        this.onReceiveAnswer(desc);
        console.log("receive remote sdp");
      } else {
        this.remoteCandidates.push(new RTCIceCandidate({ candidate: json.candidate, sdpMid: 0, sdpMLineIndex: 0 }));
        console.log("receive remote candidate");
      }
    });

    sock.addEventListener("close", e => {
      console.log("[ws-rtc] close: " + e);
    });

    sock.addEventListener("error", e => {
      console.log("[ws-rtc] error: " + e);
    });

    this.remoteCandidateTask = setInterval(() => {
      if (this.peerConnection.remoteDescription) {
        this.remoteCandidates.forEach((candidate) => {
          this.peerConnection.addIceCandidate(candidate);
        });
        this.remoteCandidates = [];
      }
    }, 1);

    this.localCandidateTask = setInterval(() => {
      if (this.sock.readyState == WebSocket.OPEN) {
        this.localCandidates.forEach((candidate) => {
          this.sock.send(JSON.stringify({ "is_candidate": false, "session": "", "sdp": "", "candidate": candidate }));
        });
        this.localCandidates = [];
      }
    }, 1);
  }

  onReceiveAnswer(desc) {
    this.peerConnection.setRemoteDescription(desc);
    console.log(`On receive answer:\n${desc.sdp}`);
  }

  onIceCandidate(pc, event) {
    if (event.candidate) {
      console.log(`ICE candidate: ${event.candidate.candidate}`);
      this.localCandidates.push(event.candidate.candidate);
    }
  }

  onAddIceCandidateSuccess() {
    console.log('AddIceCandidate success.');
  }

  onAddIceCandidateError(error) {
    console.log(`Failed to add Ice Candidate: ${error.toString()}`);
  }

  receiveChannelCallback(event) {
    console.log('Receive Channel Callback');
    this.dataChannel = event.channel;
    this.dataChannel.onmessage = this.onReceiveMessageCallback.bind(this);
    this.dataChannel.onopen = this.ondataChannelStateChange.bind(this);
    this.dataChannel.onclose = this.ondataChannelStateChange.bind(this);
  }

  onReceiveMessageCallback(event) {
    console.log('Received Message: ' + new TextDecoder().decode(event.data));
  }

  ondataChannelStateChange() {
    const readyState = this.dataChannel.readyState;
    console.log('Send channel state is: ' + readyState);

    if (readyState == "open") {
      if (this.sock != null) {
        this.sock.close();
      }

      clearInterval(this.localCandidateTask);
      clearInterval(this.remoteCandidateTask);
    }
  }
}