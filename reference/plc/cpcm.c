/* Decode length-prefixed packets with libopus and dump raw f32 PCM. */
#include <stdio.h>
#include <stdlib.h>
#include "opus.h"

int main(int argc, char **argv) {
    int rate = atoi(argv[3]), channels = atoi(argv[4]), frame = atoi(argv[5]);
    FILE *f = fopen(argv[1], "rb");
    FILE *o = fopen(argv[2], "wb");
    if (!f || !o) { perror("open"); return 2; }
    int err;
    OpusDecoder *dec = opus_decoder_create(rate, channels, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create %d\n", err); return 2; }
    float *pcm = malloc(sizeof(float) * 5760 * channels);
    unsigned char pkt[8000];
    for (;;) {
        unsigned char hdr[4];
        if (fread(hdr, 1, 4, f) != 4) break;
        int n = hdr[0] | (hdr[1]<<8) | (hdr[2]<<16) | (hdr[3]<<24);
        if ((int)fread(pkt, 1, n, f) != n) break;
        int got = opus_decode_float(dec, pkt, n, pcm, frame, 0);
        if (got < 0) { fprintf(stderr, "decode -> %d\n", got); return 2; }
        fwrite(pcm, sizeof(float), (size_t)got * channels, o);
    }
    fclose(o);
    return 0;
}
