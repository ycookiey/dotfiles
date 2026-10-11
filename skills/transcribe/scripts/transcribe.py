"""faster-whisper (large-v3-turbo) で音声・動画を文字起こし。txt / srt 出力。

GPUを優先し、cuBLAS/cuDNN の DLL (pip wheel: nvidia-cublas-cu12 / nvidia-cudnn-cu12)
を実行時にロードする。GPU不可なら CPU(int8) にフォールバック。

usage (uv経由):
  uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
    transcribe.py <path...> [--lang ja] [--model M] [--device auto|cuda|cpu] \
    [--formats txt,srt] [--overwrite] [--no-recursive]

path はファイル / ディレクトリ(再帰探索) / glob。出力は各入力と同じディレクトリに <name>.txt / <name>.srt。
既存の出力があるファイルはスキップ(冪等)。--overwrite で上書き。
"""
import sys, os, time, argparse, glob, importlib.util, contextlib

DEFAULT_MODEL = "mobiuslabsgmbh/faster-whisper-large-v3-turbo"  # HFキャッシュ利用
MEDIA_EXTS = {".mp4", ".mkv", ".mov", ".webm", ".avi", ".m4v",
              ".mp3", ".wav", ".m4a", ".aac", ".flac", ".ogg", ".opus", ".wma"}


def register_cuda_dlls():
    """pip wheel(nvidia-cublas-cu12 / nvidia-cudnn-cu12)のDLLをCTranslate2に見つけさせる(Windows)"""
    spec = importlib.util.find_spec("nvidia")
    if not spec or not spec.submodule_search_locations:
        return []
    nv = spec.submodule_search_locations[0]
    added = []
    for sub in ("cublas", "cudnn"):
        d = os.path.join(nv, sub, "bin")
        if os.path.isdir(d):
            try:
                os.add_dll_directory(d)
            except (AttributeError, OSError):
                pass
            os.environ["PATH"] = d + os.pathsep + os.environ.get("PATH", "")
            added.append(sub)
    return added


def fmt_ts(t):
    # ミリ秒の整数に丸めてから桁を分ける(秒で丸めると 59.9996 が "60,000" になる)
    ms = round(t * 1000)
    h, r = divmod(ms, 3_600_000)
    m, r = divmod(r, 60_000)
    s, ms = divmod(r, 1000)
    return f"{h:02d}:{m:02d}:{s:02d},{ms:03d}"


def collect_inputs(paths, recursive):
    """ファイル/ディレクトリ/globを展開してメディアファイル一覧へ"""
    files = []
    for p in paths:
        # 実在するパスはそのまま使う("[2024] talk.mp4" の [ を文字クラスと解釈しない)
        if os.path.exists(p) or not any(c in p for c in "*?["):
            matches = [p]
        else:
            matches = glob.glob(p)
        if not matches:
            print(f"[warn] マッチなし: {p}")
        for m in matches:
            if os.path.isdir(m):
                walker = os.walk(m) if recursive else [(m, [], os.listdir(m))]
                for root, _, names in walker:
                    for n in names:
                        if os.path.splitext(n)[1].lower() in MEDIA_EXTS:
                            files.append(os.path.join(root, n))
            elif os.path.isfile(m):
                files.append(m)
            else:
                print(f"[warn] 見つからない: {m}")
    # 重複除去・安定ソート
    seen, out = set(), []
    for f in files:
        a = os.path.abspath(f)
        if a not in seen:
            seen.add(a); out.append(f)
    return sorted(out)


def load_model(model_name, device):
    register_cuda_dlls()
    from faster_whisper import WhisperModel
    order = {"auto": (("cuda", "float16"), ("cpu", "int8")),
             "cuda": (("cuda", "float16"),),
             "cpu":  (("cpu", "int8"),)}[device]
    last = None
    for dev, ct in order:
        try:
            m = WhisperModel(model_name, device=dev, compute_type=ct)
            print(f"[model] {model_name} on {dev}/{ct}")
            return m
        except Exception as e:
            last = e
            print(f"[model] {dev} 不可: {str(e)[:120]}")
    raise SystemExit(f"モデルロード不可: {last}")


def transcribe_one(model, path, lang, formats, overwrite):
    base = os.path.splitext(path)[0]
    out_txt, out_srt = base + ".txt", base + ".srt"
    want_txt, want_srt = "txt" in formats, "srt" in formats
    if not overwrite:
        if (not want_txt or os.path.exists(out_txt)) and (not want_srt or os.path.exists(out_srt)):
            print(f"[skip] 出力済: {os.path.basename(path)}")
            return
    t0 = time.time()
    print(f"[run] {path}")
    segments, info = model.transcribe(path, language=lang, vad_filter=True, beam_size=5)
    # segments は遅延評価で、推論はループ中に進む。途中で落ちても未完の出力が
    # [skip] 出力済 と扱われないよう <name>.<ext>.part に書き、最後に置き換える。
    parts = []  # (書き込み先 .part, 最終の出力先)
    try:
        with contextlib.ExitStack() as stack:
            ft = fs = None
            if want_txt:
                parts.append((out_txt + ".part", out_txt))
                ft = stack.enter_context(open(out_txt + ".part", "w", encoding="utf-8"))
            if want_srt:
                parts.append((out_srt + ".part", out_srt))
                fs = stack.enter_context(open(out_srt + ".part", "w", encoding="utf-8"))
            n = 0
            for i, seg in enumerate(segments, 1):
                txt = seg.text.strip()
                if ft: ft.write(txt + "\n")
                if fs: fs.write(f"{i}\n{fmt_ts(seg.start)} --> {fmt_ts(seg.end)}\n{txt}\n\n")
                n = i
        for part, final in parts:
            os.replace(part, final)
    except BaseException:
        # KeyboardInterrupt も含め .part を消して再送出(呼び出し側の扱いは変えない)
        for part, _ in parts:
            try:
                os.remove(part)
            except OSError:
                pass
        raise
    el = time.time() - t0
    spd = (info.duration / el) if el else 0
    print(f"[done] {os.path.basename(path)} 音声{info.duration:.0f}s 処理{el:.0f}s ({spd:.1f}x) seg={n}")


def main():
    # パイプ経由でもブロックバッファにせず [run]/[done] を逐次表示する
    try:
        sys.stdout.reconfigure(line_buffering=True)
    except AttributeError:
        pass
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+", help="ファイル/ディレクトリ/glob")
    ap.add_argument("--lang", default="ja")
    ap.add_argument("--model", default=DEFAULT_MODEL)
    ap.add_argument("--device", default="auto", choices=["auto", "cuda", "cpu"])
    ap.add_argument("--formats", default="txt,srt", help="txt,srt のカンマ区切り")
    ap.add_argument("--overwrite", action="store_true")
    ap.add_argument("--no-recursive", dest="recursive", action="store_false")
    a = ap.parse_args()

    formats = {f.strip() for f in a.formats.split(",") if f.strip()}
    files = collect_inputs(a.paths, a.recursive)
    if not files:
        raise SystemExit("対象メディアなし")
    print(f"[plan] {len(files)} 件: " + ", ".join(os.path.basename(f) for f in files[:8])
          + (" ..." if len(files) > 8 else ""))
    model = load_model(a.model, a.device)
    for f in files:
        try:
            transcribe_one(model, f, a.lang, formats, a.overwrite)
        except Exception as e:
            print(f"[error] {os.path.basename(f)}: {str(e)[:200]}")


if __name__ == "__main__":
    main()
