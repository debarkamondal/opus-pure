/* Encode raw interleaved f32 PCM with libopus and dump opus_demo .bit framing. */
#include <stdio.h>
#include <stdlib.h>
#include <opus.h>

int main(int argc, char **argv) {
    if (argc < 7) { fprintf(stderr,"usage: cenc <pcm.f32> <out.bit> <rate> <ch> <frame> <bitrate>\n"); return 2; }
    FILE *fi = fopen(argv[1], "rb"), *fo = fopen(argv[2], "wb");
    if (!fi || !fo) { perror("open"); return 2; }
    int rate = atoi(argv[3]), ch = atoi(argv[4]), frame = atoi(argv[5]), br = atoi(argv[6]), err;
    OpusEncoder *e = opus_encoder_create(rate, ch, OPUS_APPLICATION_VOIP, &err);
    if (err != OPUS_OK) { fprintf(stderr,"create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    /* Match this crate's float-API defaults. */
    opus_encoder_ctl(e, OPUS_SET_LSB_DEPTH(24));

    float *in = malloc(sizeof(float) * frame * ch);
    unsigned char data[4000];
    int i = 0;
    while (fread(in, sizeof(float), frame * ch, fi) == (size_t)(frame * ch)) {
        int len = opus_encode_float(e, in, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr,"encode: %s\n", opus_strerror(len)); return 2; }
        opus_uint32 rng = 0;
        opus_encoder_ctl(e, OPUS_GET_FINAL_RANGE(&rng));
        unsigned char h[8] = { len>>24, len>>16, len>>8, len,
                               rng>>24, rng>>16, rng>>8, rng };
        fwrite(h, 1, 8, fo); fwrite(data, 1, len, fo);
        printf("pkt %3d len=%4d toc=0x%02x rng=0x%08x\n", i, len, data[0], rng);
        i++;
    }
    free(in); opus_encoder_destroy(e); fclose(fi); fclose(fo);
    return 0;
}
