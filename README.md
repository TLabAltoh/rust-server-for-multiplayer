# unity-rust-sfu
This is a sfu (webrtc/websocket) server intended to be integrated for [com.unity.webrtc](https://github.com/Unity-Technologies/com.unity.webrtc) and [nativewebsocket](https://github.com/endel/NativeWebSocket). This repo's webrtc implementation is mostly based on [binbat](https://github.com/binbat)'s [live777](https://github.com/binbat/live777) and [webrtc-rs](https://github.com/webrtc-rs/webrtc), but modified for use in a realtime multiplayer game. 

- Use trickle-ice for peerConnection
- broadcast and unicast

> [!WARNING]  
> I am not a member or contributor of [original source](https://github.com/binbat/live777). Please note that my modification may have dropped original source's critical features. 
