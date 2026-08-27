/* Generate reference_vectors.rs-style expected packets with fixed-point libopus.
 *
 * Mirrors `run_config` exactly: mono, VOIP, CBR, the given bitrate and
 * complexity, and the 440 Hz sine `gen_pcm` produces. Prints the packets as the
 * hex string literals the test table holds, so the output can be pasted in.
 *
 * usage: cvec <rate> <bitrate> <complexity> <bandwidth|auto> <frames>
 *   bandwidth: auto | nb | mb | wb | swb | fb
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <opus.h>

int main(int argc, char **argv) {
    if (argc < 6) { fprintf(stderr, "usage: %s <rate> <bitrate> <complexity> <auto|nb|mb|wb|swb|fb> <frames>\n", argv[0]); return 2; }
    int fs = atoi(argv[1]), br = atoi(argv[2]), cx = atoi(argv[3]), frames = atoi(argv[5]), err;
    const char *bwn = argv[4];
    int bw = strcmp(bwn,"nb")==0 ? OPUS_BANDWIDTH_NARROWBAND :
             strcmp(bwn,"mb")==0 ? OPUS_BANDWIDTH_MEDIUMBAND :
             strcmp(bwn,"wb")==0 ? OPUS_BANDWIDTH_WIDEBAND :
             strcmp(bwn,"swb")==0 ? OPUS_BANDWIDTH_SUPERWIDEBAND :
             strcmp(bwn,"fb")==0 ? OPUS_BANDWIDTH_FULLBAND : 0;

    int frame = fs / 50;
    int n = frame * frames;
    float *pcm = malloc(sizeof(float) * n);
    for (int i = 0; i < n; i++) {
        long cycle = (440L * i) % fs;                 /* integer phase reduction */
        double phase = (double)cycle / (double)fs;
        short s = (short)(sin(2.0 * M_PI * phase) * 16384.0);
        pcm[i] = (float)s / 32768.0f;
    }

    OpusEncoder *e = opus_encoder_create(fs, 1, OPUS_APPLICATION_VOIP, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_COMPLEXITY(cx));
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_VBR(0));
    if (bw) opus_encoder_ctl(e, OPUS_SET_BANDWIDTH(bw));

    unsigned char data[1275];
    int counts[32]; memset(counts, 0, sizeof counts);
    for (int i = 0; i < frames; i++) {
        int len = opus_encode_float(e, pcm + i * frame, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr, "encode: %s\n", opus_strerror(len)); return 2; }
        counts[data[0] >> 3]++;
        printf("            \"");
        for (int j = 0; j < len; j++) printf("%02x", data[j]);
        printf("\",\n");
    }
    fprintf(stderr, "configs:");
    for (int i = 0; i < 32; i++) if (counts[i]) fprintf(stderr, " %d=%d", i, counts[i]);
    fprintf(stderr, "\n");
    opus_encoder_destroy(e); free(pcm);
    return 0;
}
