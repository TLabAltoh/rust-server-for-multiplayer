class SfuWebSocket {
    constructor() {
        this.sock;
    }

    join(json) {
        this.json = json;
        this.action = "ws/connect";
        this.connect();
    }

    // https://stackoverflow.com/a/12965194/22575350
    i32ToUint8Array(int) {
        var byteArray = new Uint8Array([0, 0, 0, 0]);
        for (var index = 0; index < byteArray.length; index++) {
            var byte = int & 0xff;
            byteArray[index] = byte;
            int = (int - byte) / 256;
        }
        return byteArray;
    };

    Uint8ArrayToi32(byteArray) {
        var value = 0;
        for (var i = byteArray.length - 1; i >= 0; i--) {
            value = (value * 256) + byteArray[i];
        }
        return value;
    };

    send(message, to) {
        if (this.sock != null && this.sock.readyState === this.sock.OPEN) {
            let byteHedder = this.i32ToUint8Array(to);
            switch (typeof (message)) {
                case "string":
                    let byteMessage = new TextEncoder().encode(message);
                    this.sock.send(new Uint8Array([...byteHedder, ...byteMessage]));
                    console.log("[ws-sfu] send string");
                    break;
                case "object":
                    byteMessage = message;
                    this.sock.send(new Uint8Array([...byteHedder, ...byteMessage]));
                    break;
            }
            console.log('Send Message: ' + message);
        }
    }

    close() {
        console.log('Closing websocket');
        if (this.sock != null) {
            this.sock.close();
            this.sock = null;
        }
    }

    async connect() {
        const jsonStr = JSON.stringify(this.json);
        const jsonBase64 = btoa(jsonStr);

        console.log("json:" + jsonStr);

        const sock = new WebSocket("ws://localhost:7777/" + this.action + "/" + jsonBase64 + "/");
        this.sock = sock;
        sock.binaryType = "arraybuffer";

        sock.addEventListener("open", e => {
            console.log("[ws-sfu] open: " + e);
        });

        sock.addEventListener("message", e => {
            console.log("[ws-sfu] message: " + new TextDecoder().decode(e.data));
        });

        sock.addEventListener("close", e => {
            console.log("[ws-sfu] close: " + e);
        });

        sock.addEventListener("error", e => {
            console.log("[ws-sfu] error: " + e);
        });
    }
}