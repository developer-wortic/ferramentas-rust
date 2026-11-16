/* wbanca: mede um servidor HTTP/1.1 local com conexões mantidas (keep-alive).
 * uso: wbanca <porta> <caminho> <pedidos-por-conexao> <conexoes> [cabecalho-extra]
 * saída (jsonW-texto): {caminho:"...",pedidos:N,erros:N,segundos:..,por_segundo:..,p50_us:..,p99_us:..}
 * Cada conexão roda numa thread: manda um GET, lê a resposta inteira (Content-Length) e mede o tempo de ida e volta. */
#define _GNU_SOURCE
#include <arpa/inet.h>
#include <netinet/in.h>
#include <netinet/tcp.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <strings.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

static int porta, por_conexao;
static const char *caminho, *extra;
static long *lat; /* microssegundos de cada pedido */
static long erros;
static pthread_mutex_t trava = PTHREAD_MUTEX_INITIALIZER;

static long agora_us(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1000000L + t.tv_nsec / 1000;
}

static int conectar(void) {
    int s = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in a = {.sin_family = AF_INET, .sin_port = htons((unsigned short)porta)};
    inet_pton(AF_INET, "127.0.0.1", &a.sin_addr);
    int um = 1;
    setsockopt(s, IPPROTO_TCP, TCP_NODELAY, &um, sizeof um);
    if (connect(s, (struct sockaddr *)&a, sizeof a) < 0) { close(s); return -1; }
    return s;
}

/* lê uma resposta inteira; devolve o status ou -1 */
static int ler_resposta(int s, char *buf, size_t cap) {
    size_t n = 0;
    char *fim = NULL;
    while (!fim) {
        ssize_t r = recv(s, buf + n, cap - 1 - n, 0);
        if (r <= 0) return -1;
        n += (size_t)r;
        buf[n] = 0;
        fim = strstr(buf, "\r\n\r\n");
        if (n >= cap - 1 && !fim) return -1;
    }
    int status = atoi(buf + 9);
    long corpo = -1;
    for (char *l = buf; l && l < fim; l = strstr(l, "\r\n")) {
        if (l != buf) l += 2;
        if (!strncasecmp(l, "content-length:", 15)) { corpo = atol(l + 15); break; }
    }
    if (corpo < 0) return -1; /* sem tamanho: não dá para manter a conexão */
    size_t cabeca = (size_t)(fim + 4 - buf);
    long falta = corpo - (long)(n - cabeca);
    while (falta > 0) {
        ssize_t r = recv(s, buf, cap - 1 < (size_t)falta ? cap - 1 : (size_t)falta, 0);
        if (r <= 0) return -1;
        falta -= r;
    }
    return status;
}

static void *trabalhar(void *arg) {
    long base = (long)arg * por_conexao;
    char pedido[4096], buf[1 << 16];
    int n = snprintf(pedido, sizeof pedido, "GET %s HTTP/1.1\r\nHost: 127.0.0.1\r\n%s%sConnection: keep-alive\r\n\r\n",
                     caminho, extra ? extra : "", extra ? "\r\n" : "");
    int s = conectar();
    for (int i = 0; i < por_conexao; i++) {
        if (s < 0) s = conectar();
        long t0 = agora_us();
        int st = -1;
        if (s >= 0 && send(s, pedido, (size_t)n, MSG_NOSIGNAL) == n) st = ler_resposta(s, buf, sizeof buf);
        lat[base + i] = agora_us() - t0;
        if (st < 200 || st >= 500) {
            pthread_mutex_lock(&trava); erros++; pthread_mutex_unlock(&trava);
            if (s >= 0) close(s);
            s = -1;
        }
    }
    if (s >= 0) close(s);
    return NULL;
}

static int cmp(const void *a, const void *b) {
    long x = *(const long *)a, y = *(const long *)b;
    return (x > y) - (x < y);
}

int main(int argc, char **argv) {
    if (argc < 5) { fprintf(stderr, "uso: wbanca <porta> <caminho> <pedidos-por-conexao> <conexoes> [cabecalho]\n"); return 2; }
    porta = atoi(argv[1]); caminho = argv[2]; por_conexao = atoi(argv[3]);
    int conexoes = atoi(argv[4]);
    extra = argc > 5 ? argv[5] : NULL;
    long total = (long)por_conexao * conexoes;
    lat = calloc((size_t)total, sizeof *lat);
    pthread_t *t = calloc((size_t)conexoes, sizeof *t);
    long t0 = agora_us();
    for (long i = 0; i < conexoes; i++) pthread_create(&t[i], NULL, trabalhar, (void *)i);
    for (int i = 0; i < conexoes; i++) pthread_join(t[i], NULL);
    double seg = (double)(agora_us() - t0) / 1e6;
    qsort(lat, (size_t)total, sizeof *lat, cmp);
    printf("{caminho:\"%s\",pedidos:%ld,erros:%ld,segundos:%.3f,por_segundo:%.0f,p50_us:%ld,p99_us:%ld}\n", caminho, total, erros,
           seg, (double)total / seg, lat[total / 2], lat[total * 99 / 100]);
    return erros ? 1 : 0;
}
