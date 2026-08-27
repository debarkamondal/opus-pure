/* cenc, but with a selectable application. */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <opus.h>
int main(int argc, char **argv) {
    if (argc < 8) { fprintf(stderr,"usage: %s <pcm.f32> <out.bit> <rate> <ch> <frame> <bitrate> <voip|audio>\n", argv[0]); return 2; }
    FILE *fi = fopen(argv[1], "rb"), *fo = fopen(argv[2], "wb");
    if (!fi || !fo) { perror("open"); return 2; }
    int rate = atoi(argv[3]), ch = atoi(argv[4]), frame = atoi(argv[5]), br = atoi(argv[6]), err;
    int app = strcmp(argv[7],"audio")==0 ? OPUS_APPLICATION_AUDIO : OPUS_APPLICATION_VOIP;
    OpusEncoder *e = opus_encoder_create(rate, ch, app, &err);
    if (err != OPUS_OK) { fprintf(stderr,"create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_LSB_DEPTH(24));
    float *in = malloc(sizeof(float) * frame * ch);
    unsigned char data[4000];
    while (fread(in, sizeof(float), frame * ch, fi) == (size_t)(frame * ch)) {
        int len = opus_encode_float(e, in, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr,"encode: %s\n", opus_strerror(len)); return 2; }
        unsigned char h[8] = { len>>24, len>>16, len>>8, len, 0,0,0,0 };
        fwrite(h,1,8,fo); fwrite(data,1,len,fo);
    }
    fclose(fo); return 0;
}
