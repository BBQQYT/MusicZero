/* A native test device with the Android NDK AAudio ABI. No audio server.
 * The mock validates configuration/lifetime and can return short writes,
 * timeouts or a disconnect while recording the actual PCM sent by mz. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifdef __ANDROID__
// Cross-compilation checks these exported declarations against the real NDK.
#include <aaudio/AAudio.h>
#endif

typedef struct AAudioStreamBuilderStruct { int32_t rate, channels, format, sharing; } Builder;
typedef struct AAudioStreamStruct { Builder config; FILE *pcm; const float *next; } Stream;

static void path(char *output, const char *name) {
    snprintf(output, 4096, "%s/%s", getenv("MZ_AAUDIO_CAPTURE"), name);
}
static int marker(const char *name) {
    char filename[4096]; path(filename, name);
    FILE *file = fopen(filename, "r");
    if (!file) return 0;
    fclose(file); return 1;
}
static void event(const char *name) {
    char filename[4096]; path(filename, "events");
    FILE *file = fopen(filename, "a");
    if (!file) abort();
    fprintf(file, "%s\n", name); fclose(file);
}

int32_t AAudio_createStreamBuilder(Builder **builder) {
    event("create");
    if (marker("fail-create")) return -898;
    *builder = calloc(1, sizeof(Builder));
    return *builder ? 0 : -898;
}
void AAudioStreamBuilder_setSampleRate(Builder *b, int32_t v) { b->rate = v; }
void AAudioStreamBuilder_setChannelCount(Builder *b, int32_t v) { b->channels = v; }
void AAudioStreamBuilder_setFormat(Builder *b, int32_t v) { b->format = v; }
void AAudioStreamBuilder_setSharingMode(Builder *b, int32_t v) { b->sharing = v; }
int32_t AAudioStreamBuilder_delete(Builder *b) { event("delete-builder"); free(b); return 0; }
int32_t AAudioStreamBuilder_openStream(Builder *b, Stream **s) {
    event("open");
    if (marker("fail-open")) return -898;
    if (b->rate != 48000 || b->channels != 2 || b->format != 2 || b->sharing != 1) abort();
    *s = calloc(1, sizeof(Stream));
    if (!*s) return -898;
    (*s)->config = *b;
    char filename[4096]; path(filename, "pcm");
    (*s)->pcm = fopen(filename, "wb");
    if (!(*s)->pcm) abort();
    return 0;
}
int32_t AAudioStream_getSampleRate(Stream *s) { return s->config.rate; }
int32_t AAudioStream_getChannelCount(Stream *s) { return marker("bad-config") ? 1 : s->config.channels; }
int32_t AAudioStream_getFormat(Stream *s) { return s->config.format; }
int32_t AAudioStream_getFramesPerBurst(Stream *s) { (void)s; return 192; }
int32_t AAudioStream_setBufferSizeInFrames(Stream *s, int32_t n) { (void)s; event("buffer"); return n; }
int32_t AAudioStream_requestStart(Stream *s) { (void)s; event("start"); return marker("fail-start") ? -898 : 0; }
int32_t AAudioStream_requestStop(Stream *s) { (void)s; event("stop"); return 0; }
int32_t AAudioStream_close(Stream *s) { event("close"); fclose(s->pcm); free(s); return 0; }
const char *AAudio_convertResultToText(int32_t code) {
    return code == -899 ? "ErrorDisconnected" : "ErrorInternal";
}
int32_t AAudioStream_write(Stream *s, const void *data, int32_t frames, int64_t timeout) {
    if (frames <= 0 || frames > 480 || timeout != 100000000) abort();
    if (marker("fail-write")) return -899;
    if (marker("stall-write")) {
        struct timespec delay = { .tv_sec = 0, .tv_nsec = timeout };
        nanosleep(&delay, NULL); return 0;
    }
    int32_t written = frames;
    if (marker("partial-write")) {
        if (s->next && s->next != data) abort();
        if (written > 120) written = 120;
        s->next = written == frames ? NULL : (const float *)data + written * 2;
    }
    if (fwrite(data, sizeof(float) * 2, written, s->pcm) != (size_t)written) abort();
    fflush(s->pcm);
    return written;
}
