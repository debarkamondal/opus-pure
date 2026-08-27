/* Encode with a forced bandwidth; print the TOC config of each packet. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <opus.h>
int main(int argc, char **argv) {
    FILE *fi = fopen(argv[1], "rb");
    int rate = atoi(argv[2]), ch = atoi(argv[3]), frame = atoi(argv[4]), br = atoi(argv[5]), err;
    int app = strcmp(argv[6],"audio")==0 ? OPUS_APPLICATION_AUDIO : OPUS_APPLICATION_VOIP;
    int bw = atoi(argv[7]);
    OpusEncoder *e = opus_encoder_create(rate, ch, app, &err);
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_LSB_DEPTH(24));
    opus_encoder_ctl(e, OPUS_SET_BANDWIDTH(bw));
    float *in = malloc(sizeof(float) * frame * ch);
    unsigned char data[4000];
    int counts[32]; memset(counts, 0, sizeof counts);
    while (fread(in, sizeof(float), frame * ch, fi) == (size_t)(frame * ch)) {
        int len = opus_encode_float(e, in, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr,"encode: %s\n", opus_strerror(len)); return 2; }
        counts[data[0] >> 3]++;
    }
    for (int i = 0; i < 32; i++) if (counts[i]) printf("config%d=%d ", i, counts[i]);
    printf("\n");
    return 0;
}
