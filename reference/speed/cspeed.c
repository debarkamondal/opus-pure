/* Time libopus encode and decode over one configuration, and print the same
 * five numbers `benches/throughput.rs` prints for this crate.
 *
 * The methodology below is not a reimplementation of that benchmark's, it is a
 * transcription of it: the fastest of N passes rather than the mean, a fresh
 * encoder for every pass built outside the timed region, packets collected in a
 * separate untimed pass so no allocator work lands in the measurement, and the
 * audio read from a file the Rust side wrote so both stacks see identical
 * samples. A difference in any of those would show up as a speed difference
 * that belongs to the harness rather than to the codec.
 *
 * usage: cspeed [-d] <pcm.f32> <rate> <ch> <frame> <bitrate> <complexity>
 *               <voip|audio|lowdelay> <auto|nb|mb|wb|swb|fb> <auto|voice|music>
 *               <reps> [vbr_constraint]
 *
 * `-d` decodes only, encoding once outside the measurement, and prints just the
 * decode columns. It is `rspeed dec`'s opposite number: encoding outweighs
 * decoding by between two and eleven times in every case the table covers, so a
 * sampling profiler pointed at the full run reports almost entirely on the
 * encoder. With the flag the two stacks can be profiled over the same packets.
 *
 * `vbr_constraint` defaults to libopus's own default of 1. This crate models
 * rate control as VBR or CBR with nothing in between, so its VBR is libopus's
 * *unconstrained* VBR; pass 0 to compare against that instead of against the
 * default a C caller would get.
 * prints: enc_us_per_frame  enc_xrt  dec_us_per_frame  dec_xrt  kbps  modes
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <opus.h>

#define MAX_PACKET (1275 * 48 + 2)

static double now_s(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec + (double)t.tv_nsec * 1e-9;
}

static int pick(const char *s, const char *const *names, const int *vals, const char *what) {
    for (int i = 0; names[i]; i++)
        if (!strcmp(s, names[i])) return vals[i];
    fprintf(stderr, "%s: %s\n", what, s);
    exit(2);
}

static const char *toc_mode(unsigned char toc) {
    int cfg = toc >> 3;
    return cfg <= 11 ? "silk" : cfg <= 15 ? "hybrid" : "celt";
}

int main(int argc, char **argv) {
    /* Leading flag, so every positional argument keeps the index it had. */
    int decode_only = 0;
    if (argc > 1 && !strcmp(argv[1], "-d")) { decode_only = 1; argv++; argc--; }

    if (argc < 11) {
        fprintf(stderr, "usage: %s [-d] <pcm.f32> <rate> <ch> <frame> <bitrate> <complexity>"
                        " <voip|audio|lowdelay> <auto|nb|mb|wb|swb|fb>"
                        " <auto|voice|music> <reps>\n", argv[0]);
        return 2;
    }
    int rate = atoi(argv[2]), ch = atoi(argv[3]), frame = atoi(argv[4]);
    int bitrate = atoi(argv[5]), complexity = atoi(argv[6]), reps = atoi(argv[10]);
    int vbr_constraint = argc > 11 ? atoi(argv[11]) : 1;

    static const char *const app_n[] = {"voip", "audio", "lowdelay", 0};
    static const int app_v[] = {OPUS_APPLICATION_VOIP, OPUS_APPLICATION_AUDIO,
                                OPUS_APPLICATION_RESTRICTED_LOWDELAY};
    static const char *const bw_n[] = {"auto", "nb", "mb", "wb", "swb", "fb", 0};
    static const int bw_v[] = {OPUS_AUTO, OPUS_BANDWIDTH_NARROWBAND, OPUS_BANDWIDTH_MEDIUMBAND,
                               OPUS_BANDWIDTH_WIDEBAND, OPUS_BANDWIDTH_SUPERWIDEBAND,
                               OPUS_BANDWIDTH_FULLBAND};
    static const char *const sig_n[] = {"auto", "voice", "music", 0};
    static const int sig_v[] = {OPUS_AUTO, OPUS_SIGNAL_VOICE, OPUS_SIGNAL_MUSIC};

    int app = pick(argv[7], app_n, app_v, "application");
    int bandwidth = pick(argv[8], bw_n, bw_v, "bandwidth");
    int signal = pick(argv[9], sig_n, sig_v, "signal");

    /* Read the whole clip up front: file I/O is not part of either measurement. */
    FILE *f = fopen(argv[1], "rb");
    if (!f) { perror("pcm"); return 2; }
    fseek(f, 0, SEEK_END);
    long bytes = ftell(f);
    fseek(f, 0, SEEK_SET);
    size_t total = (size_t)bytes / sizeof(float);
    float *pcm = malloc(bytes);
    if (!pcm || fread(pcm, sizeof(float), total, f) != total) { perror("read"); return 2; }
    fclose(f);

    size_t frames = total / (size_t)ch / (size_t)frame;
    if (frames == 0) { fprintf(stderr, "clip is shorter than one frame\n"); return 2; }
    double audio_s = (double)(frames * (size_t)frame) / rate;

    unsigned char *scratch = malloc(MAX_PACKET);

    /* An encoder configured the way this crate's is. `OPUS_SET_LSB_DEPTH(24)`
     * is the float-API default on both sides; the rest are this run's case. */
    #define NEW_ENCODER(e) do {                                              \
        int err;                                                             \
        (e) = opus_encoder_create(rate, ch, app, &err);                      \
        if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; } \
        opus_encoder_ctl((e), OPUS_SET_BITRATE(bitrate));                    \
        opus_encoder_ctl((e), OPUS_SET_COMPLEXITY(complexity));              \
        opus_encoder_ctl((e), OPUS_SET_LSB_DEPTH(24));                       \
        opus_encoder_ctl((e), OPUS_SET_VBR_CONSTRAINT(vbr_constraint));      \
        if (bandwidth != OPUS_AUTO) opus_encoder_ctl((e), OPUS_SET_BANDWIDTH(bandwidth)); \
        if (signal != OPUS_AUTO) opus_encoder_ctl((e), OPUS_SET_SIGNAL(signal)); \
    } while (0)

    /* Untimed pass: keep the packets for the decode measurement, and read the
     * modes the encoder actually chose out of their TOC bytes. */
    OpusEncoder *enc;
    NEW_ENCODER(enc);
    unsigned char **packets = malloc(frames * sizeof *packets);
    int *lens = malloc(frames * sizeof *lens);
    size_t coded = 0;
    char modes[32] = "";
    for (size_t i = 0; i < frames; i++) {
        int len = opus_encode_float(enc, pcm + i * (size_t)frame * ch, frame, scratch, MAX_PACKET);
        if (len < 0) { fprintf(stderr, "encode: %s\n", opus_strerror(len)); return 2; }
        packets[i] = malloc((size_t)len);
        memcpy(packets[i], scratch, (size_t)len);
        lens[i] = len;
        coded += (size_t)len;
        const char *m = toc_mode(scratch[0]);
        if (!strstr(modes, m)) {
            if (*modes) strcat(modes, "+");
            strcat(modes, m);
        }
    }
    opus_encoder_destroy(enc);

    /* Optional 12th argument: dump the packets length-prefixed, for the split probe. */
    if (argc > 12) {
        FILE *pk = fopen(argv[12], "wb");
        if (!pk) { perror("pkt_out"); return 2; }
        for (size_t i = 0; i < frames; i++) {
            unsigned char h[4] = { lens[i], lens[i]>>8, lens[i]>>16, lens[i]>>24 };
            fwrite(h, 1, 4, pk); fwrite(packets[i], 1, (size_t)lens[i], pk);
        }
        fclose(pk);
    }

    float *out = malloc(sizeof(float) * (size_t)frame * ch);
    double enc_best = 1e30, dec_best = 1e30;
    for (int r = 0; r < reps; r++) {
        if (!decode_only) {
            NEW_ENCODER(enc);
            double t = now_s();
            /* Checked inside the timed region, as the Rust benchmark checks: an
             * error here would otherwise read as speed. */
            for (size_t i = 0; i < frames; i++)
                if (opus_encode_float(enc, pcm + i * (size_t)frame * ch, frame, scratch, MAX_PACKET) < 0)
                    { fprintf(stderr, "encode failed\n"); return 2; }
            double el = now_s() - t;
            if (el < enc_best) enc_best = el;
            opus_encoder_destroy(enc);
        }

        /* A fresh decoder per pass, built outside the timed region: decoder
         * state evolves, and the first packet into a used decoder is not the
         * same work as the first packet into a new one. */
        int err;
        OpusDecoder *dec = opus_decoder_create(rate, ch, &err);
        if (err != OPUS_OK) { fprintf(stderr, "create: %s\n", opus_strerror(err)); return 2; }
        double t = now_s();
        for (size_t i = 0; i < frames; i++)
            if (opus_decode_float(dec, packets[i], lens[i], out, frame, 0) < 0)
                { fprintf(stderr, "decode failed\n"); return 2; }
        double el = now_s() - t;
        if (el < dec_best) dec_best = el;
        opus_decoder_destroy(dec);
    }

    if (decode_only)
        printf("%.3f\t%.1f\t%s\n",
               dec_best * 1e6 / (double)frames, audio_s / dec_best, modes);
    else
        printf("%.3f\t%.1f\t%.3f\t%.1f\t%.1f\t%s\n",
               enc_best * 1e6 / (double)frames, audio_s / enc_best,
               dec_best * 1e6 / (double)frames, audio_s / dec_best,
               (double)coded * 8.0 / audio_s / 1000.0, modes);
    return 0;
}
