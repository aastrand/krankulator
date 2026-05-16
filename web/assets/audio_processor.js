class NesAudioProcessor extends AudioWorkletProcessor {
    constructor() {
        super();
        this.ring = new Float32Array(8192);
        this.writePos = 0;
        this.readPos = 0;
        this.count = 0;
        this.reportCounter = 0;

        this.port.onmessage = (e) => {
            const samples = e.data;
            for (let i = 0; i < samples.length; i++) {
                if (this.count < this.ring.length) {
                    this.ring[this.writePos] = samples[i];
                    this.writePos = (this.writePos + 1) & (this.ring.length - 1);
                    this.count++;
                }
            }
        };
    }

    process(inputs, outputs) {
        const output = outputs[0][0];
        for (let i = 0; i < output.length; i++) {
            if (this.count > 0) {
                output[i] = this.ring[this.readPos];
                this.readPos = (this.readPos + 1) & (this.ring.length - 1);
                this.count--;
            } else {
                output[i] = 0;
            }
        }
        this.reportCounter++;
        if (this.reportCounter >= 8) {
            this.port.postMessage(this.count);
            this.reportCounter = 0;
        }
        return true;
    }
}

registerProcessor("nes-audio-processor", NesAudioProcessor);
