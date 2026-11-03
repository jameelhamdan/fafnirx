"""Scaling benchmark of the master in Docker: per configuration a fresh master and PostgreSQL pinned to cores and memory, then
ingest, reads, thumbnails, uploads and a download. Appends one JSON line each. Standard library only.

    python bench_docker.py <library> <results.jsonl> name=MASTER_CPUSET:PG_CPUSET[,ENV=value...] ...
    python bench_docker.py lib out.jsonl c1=0:0 rpi=0-1:0-1,MASTER_MEM=640m,PG_MEM=320m,FAFNIRX_DB_POOL=4

<library> holds tiny/ and large/ made by bench-library.py. Needs the image: docker build -f docker/Dockerfile.master -t fafnirx:bench .
"""
import http.client
import json
import os
import re
import subprocess
import sys
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
HOST, PORT = '127.0.0.1', 18080
MiB = 1 << 20
HDR = {'x-requested-with': 'bench'}
cookie = ''


def sh(*args, env=None):
    return subprocess.run(args, capture_output=True, text=True, env={**os.environ, **(env or {})})


class Conn(threading.local):
    """One keep-alive connection per thread."""
    c = None


_tl = Conn()


def request(method, path, body=None, headers=None, raw=False):
    h = {**HDR, 'cookie': cookie, **(headers or {})}
    if isinstance(body, (dict, list)):
        body, h['content-type'] = json.dumps(body), 'application/json'
    for attempt in (0, 1):
        if _tl.c is None:
            _tl.c = http.client.HTTPConnection(HOST, PORT, timeout=120)
        try:
            _tl.c.request(method, path, body=body, headers=h)
            r = _tl.c.getresponse()
            data = r.read()
            return r.status, (data if raw else (json.loads(data) if data[:1] in b'{[' else data)), r
        except (http.client.HTTPException, OSError):
            _tl.c.close()
            _tl.c = None
            if attempt:
                raise


def get(path, **kw):
    return request('GET', path, **kw)


def pct(a, p):
    return a[min(len(a) - 1, int(p / 100 * len(a)))]


def mib(text):
    m = re.match(r'([\d.]+)(KiB|MiB|GiB)', text or '')
    return float(m[1]) * {'KiB': 1 / 1024, 'MiB': 1, 'GiB': 1024}[m[2]] if m else 0


class Watch:
    """CPU (100% = one core) and memory of the master and PostgreSQL containers, sampled until stop()."""

    def __init__(self):
        self.s = {'master': [], 'db': [], 'mm': [], 'dm': []}
        self.on = True
        self.t = threading.Thread(target=self.loop, daemon=True)
        self.t.start()

    def loop(self):
        while self.on:
            out = sh('docker', 'stats', '--no-stream', '--format', '{{.Name}} {{.CPUPerc}} {{.MemUsage}}', 'fxbench-master-1', 'fxbench-db-1').stdout
            for line in out.strip().splitlines():
                name, cpu, rest = (line.split(' ', 2) + ['', ''])[:3]
                try:
                    v = float(cpu.rstrip('%'))
                except ValueError:
                    continue
                k = 'master' if 'master' in name else 'db'
                self.s[k].append(v)
                self.s['mm' if k == 'master' else 'dm'].append(mib(rest))

    def stop(self):
        self.on = False
        self.t.join()
        f = lambda a: {'mean': round(sum(a) / len(a) / 100, 2), 'peak': round(max(a) / 100, 2)} if a else {'mean': 0, 'peak': 0}
        return {'masterCores': f(self.s['master']), 'pgCores': f(self.s['db']),
                'masterPeakMiB': round(max(self.s['mm'], default=0)), 'pgPeakMiB': round(max(self.s['dm'], default=0))}


def concurrent(path, c, seconds):
    times, failed = [], [0]
    lock = threading.Lock()
    end = time.time() + seconds

    def worker():
        mine, bad = [], 0
        while time.time() < end:
            t = time.perf_counter()
            try:
                st, _, _ = get(path, raw=True)
                bad += st >= 400
            except Exception:
                bad += 1
            mine.append((time.perf_counter() - t) * 1000)
        with lock:
            times.extend(mine)
            failed[0] += bad

    t0 = time.time()
    ts = [threading.Thread(target=worker) for _ in range(c)]
    [t.start() for t in ts]
    [t.join() for t in ts]
    times.sort()
    return {'rps': round(len(times) / (time.time() - t0)), 'p50': round(pct(times, 50), 1), 'p99': round(pct(times, 99), 1), 'failed': failed[0]}


def drain(expected):
    t0 = time.time()
    quiet = 0
    while time.time() - t0 < 20 * 60:
        q = get('/api/queue')[1]
        busy = sum(s['queued'] + s['running'] for s in (q.get('summary') or []))
        total = get('/api/timeline/buckets')[1].get('total', 0)
        quiet = quiet + 1 if busy == 0 and total >= expected else 0
        if quiet >= 2:
            break
        time.sleep(0.5)
    return time.time() - t0


def upload(rel, size, chunk_size):
    uid = request('POST', '/api/uploads', {'device': 'admin/up', 'path': rel, 'size': size})[1]['id']
    chunk = b'\x03' * min(chunk_size, size)
    off = 0
    while off < size:
        body = chunk if off + len(chunk) <= size else chunk[:size - off]
        st, _, _ = request('PUT', f'/api/uploads/{uid}?offset={off}', body, raw=True)
        if st >= 300:
            raise RuntimeError(f'chunk {st}')
        off += len(body)
    st = request('POST', f'/api/uploads/{uid}/finish', {}, raw=True)[0]
    if st != 200:
        raise RuntimeError(f'finish {st}')


def parallel(n, fn):
    ts = [threading.Thread(target=fn, args=(i,)) for i in range(n)]
    [t.start() for t in ts]
    [t.join() for t in ts]


def run_config(spec, lib, out):
    global cookie
    name, rest = spec.split('=', 1)
    cpus, *envs = rest.split(',')
    master_cpus, pg_cpus = cpus.split(':')
    env = {'MASTER_CPUSET': master_cpus, 'PG_CPUSET': pg_cpus, **dict(e.split('=', 1) for e in envs)}
    compose = lambda *a: sh('docker', 'compose', '-f', os.path.join(HERE, 'docker-compose.bench.yml'), *a, env=env)
    compose('down', '-v', '--remove-orphans')
    up = compose('up', '-d', '--wait')
    if up.returncode:
        raise RuntimeError(up.stderr)
    res = {'name': name, **env}
    _tl.c = None
    try:
        r = http.client.HTTPConnection(HOST, PORT)
        r.request('POST', '/api/login', json.dumps({'username': 'admin', 'password': 'pw'}), {**HDR, 'content-type': 'application/json'})
        cookie = r.getresponse().getheader('set-cookie').split(';')[0]
        sh('docker', 'exec', '-u', '1000', 'fxbench-master-1', 'mkdir', '-p', '/data/files/users/admin/lib')
        for kind in ('tiny', 'large'):
            n = int(sh('sh', '-c', f'find {lib}/{kind} -type f | wc -l').stdout)
            before = get('/api/timeline/buckets')[1]['total']
            w = Watch()
            cp = sh('docker', 'cp', f'{lib}/{kind}', 'fxbench-master-1:/data/files/users/admin/lib/')
            if cp.returncode:
                raise RuntimeError(cp.stderr)
            secs = drain(before + n)
            res['ingest' + kind.capitalize()] = {'files': n, 'seconds': round(secs, 1), 'filesPerSec': round(n / secs, 1), **w.stop()}
        months = get('/api/timeline/buckets')[1]['months']
        res['reads'] = {}
        for label, path in [('healthz', '/healthz'), ('buckets', '/api/timeline/buckets'), ('dashboard', '/api/dashboard'),
                            ('month', f'/api/timeline?month={months[0]["month"]}'), ('files', '/api/files?device=admin%2Flib')]:
            res['reads'][label] = {}
            for c in (1, 4, 16, 64, 256):
                w = Watch()
                r = concurrent(path, c, 6)
                res['reads'][label][c] = {**r, **w.stop()}
        assets = []
        for m in months:
            assets += [a for a in get(f'/api/timeline?month={m["month"]}')[1]['assets'] if a['size'] > MiB]
        res['thumbs'] = {}
        at = 0
        for c, n in ((1, 20), (4, 40), (16, max(0, len(assets) - 60))):
            batch = assets[at:at + n]
            at += n
            if not batch:
                continue
            nxt, lock, codes = [0], threading.Lock(), {}

            def lane(_):
                while True:
                    with lock:
                        i = nxt[0]
                        nxt[0] += 1
                    if i >= len(batch):
                        return
                    a = batch[i]
                    # 202 means the picture is being made: ask again until it is there.
                    for _ in range(2400):
                        st = get(f'/api/media/{a["id"]}/thumb?v={a["hash"]}', raw=True)[0]
                        if st != 202:
                            break
                        time.sleep(0.05)
                    with lock:
                        codes[st] = codes.get(st, 0) + 1

            w = Watch()
            t0 = time.time()
            parallel(c, lane)
            res['thumbs'][c] = {'n': len(batch), 'perSec': round(len(batch) / (time.time() - t0), 1), 'status': codes, **w.stop()}
        request('POST', '/api/devices', {'name': 'up', 'kind': 'local'}, raw=True)
        w = Watch()
        t0 = time.time()
        upload('big.bin', 256 * MiB, 32 * MiB)
        res['uploadBigMiBs'] = {'value': round(256 / (time.time() - t0)), **w.stop()}
        w = Watch()
        t0 = time.time()
        parallel(4, lambda lane: [upload(f'small/f{i}.bin', 64 * 1024, 64 * 1024) for i in range(lane, 200, 4)])
        res['uploadSmallPerSec'] = {'value': round(200 / (time.time() - t0)), **w.stop()}
        w = Watch()
        t0 = time.time()
        got = len(get('/api/files/download?device=admin%2Fup&path=big.bin', raw=True)[1])
        res['downloadMiBs'] = {'value': round(got / MiB / (time.time() - t0)), **w.stop()}
    except Exception as e:  # a master killed for memory ends up here
        res['error'] = repr(e)
    finally:
        for c in ('master', 'db'):
            o = sh('docker', 'inspect', '-f', '{{.State.OOMKilled}} {{.State.Status}}', f'fxbench-{c}-1').stdout.strip()
            if o.startswith('true') or o.endswith('exited'):
                res.setdefault('killed', []).append(f'{c}: {o}')
        compose('down', '-v', '--remove-orphans')
    with open(out, 'a') as f:
        f.write(json.dumps(res) + '\n')
    print(json.dumps(res), flush=True)


if __name__ == '__main__':
    lib, out, *configs = sys.argv[1:]
    for spec in configs:
        run_config(spec, lib, out)
