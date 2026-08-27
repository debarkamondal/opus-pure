/* Decode length-prefixed packets with libopus, dropping a given set of them,
   and dump raw f32 PCM. The point is a sample-for-sample comparison of the
   concealed frames themselves, which per-frame energy (cdec.c) cannot make.

   usage: cplc <pkt_in> <pcm_out> <rate> <channels> <frame_samples> [lost_idx,...]
*/
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "opus.h"

#define MAX_LOST 4096

int main(int argc, char **argv) {
    if (argc < 6) {
        fprintf(stderr, "usage: %s <pkt_in> <pcm_out> <rate> <ch> <frame> [lost,...]\n", argv[0]);
        return 2;
    }
    int rate = atoi(argv[3]), channels = atoi(argv[4]), frame = atoi(argv[5]);
    int lost[MAX_LOST], n_lost = 0;
    if (argc > 6 && argv[6][0]) {
        char *s = argv[6], *tok;
        for (tok = strtok(s, ","); tok && n_lost < MAX_LOST; tok = strtok(NULL, ","))
            lost[n_lost++] = atoi(tok);
    }

    FILE *f = fopen(argv[1], "rb");
    FILE *o = fopen(argv[2], "wb");
    if (!f || !o) { perror("open"); return 2; }
    int err;
    OpusDecoder *dec = opus_decoder_create(rate, channels, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create %d\n", err); return 2; }

    float *pcm = malloc(sizeof(float) * 5760 * channels);
    unsigned char pkt[8000];
    int idx = 0;
    for (;;) {
        unsigned char hdr[4];
        if (fread(hdr, 1, 4, f) != 4) break;
        int n = hdr[0] | (hdr[1]<<8) | (hdr[2]<<16) | (hdr[3]<<24);
        if ((int)fread(pkt, 1, n, f) != n) break;
        int drop = 0;
        for (int i = 0; i < n_lost; i++) if (lost[i] == idx) drop = 1;
        int got = drop ? opus_decode_float(dec, NULL, 0, pcm, frame, 0)
                       : opus_decode_float(dec, pkt, n, pcm, frame, 0);
        if (got < 0) { fprintf(stderr, "frame %d: decode -> %d\n", idx, got); return 2; }
        fwrite(pcm, sizeof(float), (size_t)got * channels, o);
        idx++;
    }
    fclose(o);
    fprintf(stderr, "%d packets, %d concealed\n", idx, n_lost);
    return 0;
}
