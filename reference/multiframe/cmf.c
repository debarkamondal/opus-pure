/* What mode and framing does libopus pick for a given duration? Prints one line
   per packet: length, TOC, mode, frame count. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <opus.h>

static const char *mode_of(unsigned char toc) {
    int cfg = toc >> 3;
    if (cfg >= 16) return "celt";
    if (cfg >= 12) return "hybrid";
    return "silk";
}

int main(int argc, char **argv) {
    if (argc < 8) { fprintf(stderr,"usage: cmf <pcm.f32> <rate> <ch> <frame> <bitrate> <app:voip|audio> <vbr:0|1>\n"); return 2; }
    FILE *fi = fopen(argv[1], "rb");
    if (!fi) { perror("open"); return 2; }
    int rate = atoi(argv[2]), ch = atoi(argv[3]), frame = atoi(argv[4]), br = atoi(argv[5]), err;
    int app = strcmp(argv[6], "audio") == 0 ? OPUS_APPLICATION_AUDIO : OPUS_APPLICATION_VOIP;
    int vbr = atoi(argv[7]);
    OpusEncoder *e = opus_encoder_create(rate, ch, app, &err);
    if (err != OPUS_OK) { fprintf(stderr,"create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_VBR(vbr));
    opus_encoder_ctl(e, OPUS_SET_LSB_DEPTH(24));

    float *in = malloc(sizeof(float) * frame * ch);
    unsigned char data[8000];
    int i = 0;
    while (fread(in, sizeof(float), frame * ch, fi) == (size_t)(frame * ch)) {
        int n = opus_encode_float(e, in, frame, data, sizeof(data));
        if (n < 0) { fprintf(stderr,"encode: %s\n", opus_strerror(n)); return 2; }
        printf("%3d len=%5d toc=%02x mode=%-6s code=%d frames=%d\n",
               i, n, data[0], mode_of(data[0]), data[0] & 3,
               opus_packet_get_nb_frames(data, n));
        i++;
    }
    opus_encoder_destroy(e);
    free(in);
    return 0;
}
