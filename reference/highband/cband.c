/* Decode length-prefixed packets with libopus and write float PCM.
 *
 * The mirror of `split`, which does the same with this crate's decoder. Having
 * both is what separates "libopus's encoder chose a quieter high band" from
 * "our decoder gets libopus's high band wrong": run the same packets through
 * each decoder and the difference between the two answers is ours.
 *
 * usage: cband <pkt> <rate> <ch> <frame> <pcm_out>
 */
#include <stdio.h>
#include <stdlib.h>
#include "opus.h"

int main(int argc, char **argv) {
    if (argc < 6) {
        fprintf(stderr, "usage: %s <pkt> <rate> <ch> <frame> <pcm_out>\n", argv[0]);
        return 2;
    }
    int rate = atoi(argv[2]), ch = atoi(argv[3]), frame = atoi(argv[4]);
    FILE *f = fopen(argv[1], "rb");
    if (!f) { perror("pkt"); return 2; }
    FILE *o = fopen(argv[5], "wb");
    if (!o) { perror("pcm_out"); return 2; }

    int err;
    OpusDecoder *dec = opus_decoder_create(rate, ch, &err);
    if (err != OPUS_OK) { fprintf(stderr, "decoder_create %d\n", err); return 2; }

    float *pcm = malloc(sizeof(float) * 5760 * ch);
    unsigned char pkt[8000];
    long frames = 0;
    for (;;) {
        unsigned char hdr[4];
        if (fread(hdr, 1, 4, f) != 4) break;
        int n = hdr[0] | (hdr[1] << 8) | (hdr[2] << 16) | (hdr[3] << 24);
        if (n < 0 || n > (int)sizeof pkt || (int)fread(pkt, 1, n, f) != n) break;
        int got = opus_decode_float(dec, pkt, n, pcm, frame, 0);
        if (got < 0) { fprintf(stderr, "frame %ld: decode -> %d\n", frames, got); return 2; }
        fwrite(pcm, sizeof(float), (size_t)got * ch, o);
        frames++;
    }
    printf("%ld frames -> %s\n", frames, argv[5]);
    free(pcm);
    opus_decoder_destroy(dec);
    fclose(o);
    fclose(f);
    return 0;
}
