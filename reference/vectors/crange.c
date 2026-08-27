/* Decode an opus_demo .bit stream with libopus and print the decoder's final
   range beside the encoder's stored one. */
#include <stdio.h>
#include <stdlib.h>
#include <opus.h>

static unsigned int be32(const unsigned char *p) {
    return ((unsigned)p[0]<<24)|((unsigned)p[1]<<16)|((unsigned)p[2]<<8)|p[3];
}

int main(int argc, char **argv) {
    if (argc < 4) { fprintf(stderr, "usage: crange <bit> <rate> <channels>\n"); return 2; }
    FILE *f = fopen(argv[1], "rb");
    if (!f) { perror("open"); return 2; }
    int rate = atoi(argv[2]), ch = atoi(argv[3]), err;
    OpusDecoder *dec = opus_decoder_create(rate, ch, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }

    unsigned char hdr[8], data[8192];
    float *pcm = malloc(sizeof(float) * 5760 * ch);
    int i = 0, bad = 0;
    while (fread(hdr, 1, 8, f) == 8) {
        unsigned len = be32(hdr), enc_rng = be32(hdr + 4);
        if (len > sizeof data) { fprintf(stderr, "len %u too big\n", len); break; }
        if (fread(data, 1, len, f) != len) break;
        int n = opus_decode_float(dec, data, len, pcm, 5760, 0);
        opus_uint32 dec_rng = 0;
        opus_decoder_ctl(dec, OPUS_GET_FINAL_RANGE(&dec_rng));
        const char *flag = (dec_rng == enc_rng) ? "" : "   <-- C DISAGREES WITH STORED";
        printf("pkt %3d len=%4u n=%4d stored_rng=0x%08x  c_dec_rng=0x%08x%s\n",
               i, len, n, enc_rng, dec_rng, flag);
        if (dec_rng != enc_rng) bad++;
        i++;
    }
    printf("packets=%d mismatches=%d\n", i, bad);
    free(pcm); opus_decoder_destroy(dec); fclose(f);
    return 0;
}
