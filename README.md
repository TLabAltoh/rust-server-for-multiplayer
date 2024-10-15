# unity-rust-sfu
This is an SFU server for relaying [WebRTC](https://webrtc.org/?hl=en) and [WebSocket](https://developer.mozilla.org/en-US/docs/Web/API/WebSockets_API) messages. Intended for use in a realtime multiplayer game. This repository's webrtc implementation is mostly based on [binbat](https://github.com/binbat)'s [live777](https://github.com/binbat/live777) and [webrtc-rs](https://github.com/webrtc-rs/webrtc). 

- Use [trickle-ice](https://webrtc.github.io/samples/src/content/peerconnection/trickle-ice/) for WebRTC
- ```broadcast``` and ```unicast``` (```multicast``` isn't yet)

> [!WARNING]  
> I am not a member or contributor of [original source](https://github.com/binbat/live777). Please note that my modification may have dropped original source's critical features. 
