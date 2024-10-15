class SfuClient {
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
}