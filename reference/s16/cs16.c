/* Reference generator for the integer-PCM API (tests/integer_pcm.rs).
 *
 *   cs16 enc  <in.s16> <out.pkt> <rate> <ch> <frame> <bitrate> <complexity> <bw> <signal>
 *   cs16 dec  <in.pkt> <out.s16> <rate> <ch> <frame>
 *   cs16 clip <in.f32> <out.f32> <ch> <block>
 *
 * `.pkt` is a stream of [u32 LE length][payload]. Every hash printed is
 * FNV-1a over the output bytes, which is what the Rust test compares.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <opus.h>

static unsigned long long fnv1a(const void *buf, size_t n) {
    const unsigned char *p = buf;
    unsigned long long h = 14695981039346656037ULL;
    for (size_t i = 0; i < n; i++) { h ^= p[i]; h *= 1099511628211ULL; }
    return h;
}

static void *slurp(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); exit(2); }
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    void *b = malloc(n ? n : 1);
    if (fread(b, 1, n, f) != (size_t)n) { perror("read"); exit(2); }
    fclose(f); *len = n; return b;
}

static int do_enc(char **a) {
    size_t n; short *pcm = slurp(a[0], &n);
    int rate = atoi(a[2]), ch = atoi(a[3]), frame = atoi(a[4]);
    int br = atoi(a[5]), cx = atoi(a[6]), bw = atoi(a[7]), sig = atoi(a[8]), err;
    OpusEncoder *e = opus_encoder_create(rate, ch, OPUS_APPLICATION_VOIP, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_VBR(0));
    opus_encoder_ctl(e, OPUS_SET_COMPLEXITY(cx));
    if (bw) opus_encoder_ctl(e, OPUS_SET_BANDWIDTH(bw));
    if (sig) opus_encoder_ctl(e, OPUS_SET_SIGNAL(sig));
    FILE *fo = fopen(a[1], "wb");
    unsigned char data[4000];
    size_t frames = (n / sizeof(short)) / (size_t)(frame * ch);
    unsigned char *all = malloc(frames * sizeof data); size_t tot = 0;
    for (size_t i = 0; i < frames; i++) {
        int len = opus_encode(e, pcm + i * frame * ch, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr, "encode: %s\n", opus_strerror(len)); return 2; }
        unsigned char hdr[4] = { len & 255, (len >> 8) & 255, (len >> 16) & 255, (len >> 24) & 255 };
        fwrite(hdr, 1, 4, fo); fwrite(data, 1, len, fo);
        memcpy(all + tot, data, len); tot += len;
    }
    fclose(fo);
    printf("enc frames=%zu bytes=%zu hash=%016llx\n", frames, tot, fnv1a(all, tot));
    return 0;
}

static int do_encf(char **a) {
    size_t n; float *pcm = slurp(a[0], &n);
    int rate = atoi(a[2]), ch = atoi(a[3]), frame = atoi(a[4]);
    int br = atoi(a[5]), cx = atoi(a[6]), bw = atoi(a[7]), depth = atoi(a[8]), err;
    OpusEncoder *e = opus_encoder_create(rate, ch, OPUS_APPLICATION_VOIP, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }
    opus_encoder_ctl(e, OPUS_SET_BITRATE(br));
    opus_encoder_ctl(e, OPUS_SET_VBR(0));
    opus_encoder_ctl(e, OPUS_SET_COMPLEXITY(cx));
    opus_encoder_ctl(e, OPUS_SET_LSB_DEPTH(depth));
    if (bw) opus_encoder_ctl(e, OPUS_SET_BANDWIDTH(bw));
    FILE *fo = fopen(a[1], "wb");
    unsigned char data[4000];
    size_t frames = (n / sizeof(float)) / (size_t)(frame * ch);
    unsigned char *all = malloc(frames * sizeof data); size_t tot = 0;
    for (size_t i = 0; i < frames; i++) {
        int len = opus_encode_float(e, pcm + i * frame * ch, frame, data, sizeof data);
        if (len < 0) { fprintf(stderr, "encode: %s\n", opus_strerror(len)); return 2; }
        unsigned char hdr[4] = { len & 255, (len >> 8) & 255, (len >> 16) & 255, (len >> 24) & 255 };
        fwrite(hdr, 1, 4, fo); fwrite(data, 1, len, fo);
        memcpy(all + tot, data, len); tot += len;
    }
    fclose(fo);
    printf("encf frames=%zu bytes=%zu hash=%016llx\n", frames, tot, fnv1a(all, tot));
    return 0;
}

static int do_dec(char **a) {
    size_t n; unsigned char *pkt = slurp(a[0], &n);
    int rate = atoi(a[2]), ch = atoi(a[3]), frame = atoi(a[4]), err;
    OpusDecoder *d = opus_decoder_create(rate, ch, &err);
    if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }
    FILE *fo = fopen(a[1], "wb");
    short *out = malloc(sizeof(short) * frame * ch);
    short *all = malloc(n * 64); size_t tot = 0, count = 0;
    size_t off = 0;
    while (off + 4 <= n) {
        unsigned len = pkt[off] | (pkt[off+1] << 8) | (pkt[off+2] << 16) | ((unsigned)pkt[off+3] << 24);
        off += 4;
        if (off + len > n) break;
        int got = opus_decode(d, pkt + off, len, out, frame, 0);
        if (got < 0) { fprintf(stderr, "decode: %s\n", opus_strerror(got)); return 2; }
        off += len;
        fwrite(out, sizeof(short), (size_t)got * ch, fo);
        memcpy(all + tot, out, sizeof(short) * (size_t)got * ch);
        tot += (size_t)got * ch; count++;
    }
    fclose(fo);
    printf("dec packets=%zu samples=%zu hash=%016llx\n", count, tot,
           fnv1a(all, tot * sizeof(short)));
    return 0;
}

static int do_clip(char **a) {
    size_t n; float *x = slurp(a[0], &n);
    int ch = atoi(a[2]), block = atoi(a[3]);
    size_t total = n / sizeof(float);
    float mem[8]; memset(mem, 0, sizeof mem);
    for (size_t off = 0; off + (size_t)(block * ch) <= total; off += (size_t)(block * ch))
        opus_pcm_soft_clip(x + off, block, ch, mem);
    FILE *fo = fopen(a[1], "wb");
    fwrite(x, sizeof(float), total, fo);
    fclose(fo);
    printf("clip samples=%zu hash=%016llx\n", total, fnv1a(x, total * sizeof(float)));
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) { fprintf(stderr, "usage: cs16 enc|dec|clip ...\n"); return 2; }
    if (!strcmp(argv[1], "enc") && argc == 11) return do_enc(argv + 2);
    if (!strcmp(argv[1], "encf") && argc == 11) return do_encf(argv + 2);
    if (!strcmp(argv[1], "dec") && argc == 7) return do_dec(argv + 2);
    if (!strcmp(argv[1], "clip") && argc == 6) return do_clip(argv + 2);
    fprintf(stderr, "bad arguments\n");
    return 2;
}
