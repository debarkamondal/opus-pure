/* Decode length-prefixed packets with libopus, concealing a run of losses,
   and print per-frame energy so the Rust decoder can be held against it. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "opus.h"

int main(int argc, char **argv) {
    int rate = atoi(argv[2]), frame = atoi(argv[3]);
    int lose_from = atoi(argv[4]), lose_count = atoi(argv[5]);
    FILE *f = fopen(argv[1], "rb");
    if (!f) { perror("open"); return 2; }
    int err;
    OpusDecoder *dec = opus_decoder_create(rate, 1, &err);
    if (err != OPUS_OK) { printf("decoder_create %d\n", err); return 2; }

    float *pcm = malloc(sizeof(float) * 5760);
    unsigned char pkt[8000];
    int idx = 0;
    for (;;) {
        unsigned char hdr[4];
        if (fread(hdr, 1, 4, f) != 4) break;
        int n = hdr[0] | (hdr[1]<<8) | (hdr[2]<<16) | (hdr[3]<<24);
        if ((int)fread(pkt, 1, n, f) != n) break;
        int lost = (idx >= lose_from && idx < lose_from + lose_count);
        int got = lost ? opus_decode_float(dec, NULL, 0, pcm, frame, 0)
                       : opus_decode_float(dec, pkt, n, pcm, frame, 0);
        if (got < 0) { printf("frame %d: decode -> %d\n", idx, got); return 2; }
        double e = 0;
        for (int i = 0; i < got; i++) e += (double)pcm[i] * pcm[i];
        e /= got;
        printf("%d %s %.9f\n", idx, lost ? "LOST" : "ok  ", e);
        idx++;
    }
    return 0;
}
