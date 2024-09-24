class SfuPeerConnection {
  constructor() {
    this.peerConnection;
    this.dataChannel;
  }

  buffer_to_string(buf) {
    return String.fromCharCode.apply("", new Uint8Array(buf))
  }

  whip(json) {
    this.json = json;
    this.action = "stream/whip";

    const servers = {
      offerToReceiveAudio: 1,
      offerToReceiveVideo: 1,
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

    this.peerConnection.createOffer().then(
      this.onCreateOffer.bind(this),
      this.onCreateSessionDescriptionError.bind(this)
    );
  }

  whep(json) {
    this.json = json;
    this.action = "stream/whep";

    const servers = {
      offerToReceiveAudio: 1,
      offerToReceiveVideo: 1,
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

    this.peerConnection.createOffer().then(
      this.onCreateOffer.bind(this),
      this.onCreateSessionDescriptionError.bind(this)
    );
  }

  onCreateSessionDescriptionError(error) {
    console.log('Failed to create session description: ' + error.toString());
  }

  sendData(data) {
    this.dataChannel.send(data);
    console.log('Sent Data: ' + data);
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

    const sock = new WebSocket("ws://192.168.3.9:7777/" + this.action + "/" + jsonBase64 + "/");
    this.sock = sock;

    sock.addEventListener("open", e => {
      console.log("[ws] open: " + e);
    });

    sock.addEventListener("message", e => {
      console.log("[ws] message: " + e);
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
      console.log("[ws] close: " + e);
    });

    sock.addEventListener("error", e => {
      console.log("[ws] error: " + e);
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
          this.sock.send(JSON.stringify({ "is_candidate": false, "sdp": "", "candidate": candidate }));
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
    console.log('Received Message');
  }

  ondataChannelStateChange() {
    const readyState = this.dataChannel.readyState;
    console.log('Send channel state is: ' + readyState);

    if (readyState == "open") {
      clearInterval(this.localCandidateTask);
      clearInterval(this.remoteCandidateTask);
    }
  }
}