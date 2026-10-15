---
name: transcribe
description: "音声・動画ファイルをfaster-whisper(large-v3-turbo)で文字起こしし、txt/srtを出力する。GPU優先・CPUフォールバック。録音・録画済みの音声・動画ファイル(mp4/mp3/wav/m4a/mkv/mov等)を文字にしたい・字幕(srt)を作りたいときに使う。"
allowed-tools: Bash, Read, Glob
---

# transcribe — 音声・動画の文字起こし

faster-whisper の `large-v3-turbo` モデルで、音声・動画ファイルを文字起こしする。
各入力と同じディレクトリに `<name>.txt`（本文）と `<name>.srt`（字幕）を出力する。

## 実行コマンド

シェルは Git Bash。`uv` で依存を都度供給して実行する（環境構築不要）。

```bash
uv run --python 3.12 \
  --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" \
  "<対象パス>" ["<対象パス2>" ...] [オプション]
```

- 対象パスは **ファイル / ディレクトリ(再帰探索) / glob** のいずれも可。複数指定可
- ディレクトリ指定時は配下の対応拡張子(mp4/mkv/mov/webm/avi/mp3/wav/m4a/aac/flac/ogg/opus 等)を再帰収集
- 日本語ファイル名・スペース・`【】`を含むパスは **必ずダブルクオート** で囲む

### オプション
| オプション | 既定 | 説明 |
|---|---|---|
| `--lang` | `ja` | 言語コード |
| `--model` | `mobiuslabsgmbh/faster-whisper-large-v3-turbo` | モデル(HFキャッシュ利用) |
| `--device` | `auto` | `auto`(cuda→cpu) / `cuda` / `cpu` |
| `--formats` | `txt,srt` | 出力形式。`txt` のみ等も可 |
| `--overwrite` | off | 既存出力を上書き（既定はスキップ＝冪等） |
| `--no-recursive` | off | ディレクトリを再帰探索しない |

## 重要な注意

- **GPU**: cuBLAS/cuDNN の DLL を pip wheel から供給し、スクリプトが実行時に `os.add_dll_directory` で登録する（CUDA Toolkit のインストール不要）。RTX系GPUで実時間の **約20倍速**。GPUが使えない環境では自動で **CPU(int8)** にフォールバック
- **初回のみ重いDL**: `nvidia-cudnn-cu12`(~655MB)・`nvidia-cublas-cu12`(~527MB) のwheelと、モデル本体をダウンロードする。以降は uv / HF キャッシュから即時
- **音声の読み込み**: `.wav` は Python 標準の `wave`、それ以外は `ffmpeg` CLI で 16kHz モノラルにしてからモデルに渡す。faster-whisper 内蔵の読み込み(PyAV)は、PyAV の版によって `open() got an unexpected keyword argument 'metadata_errors'` で失敗するため通さない。`ffmpeg` が PATH に無いときだけ内蔵の読み込みに任せる(警告を出す)
- **冪等**: 既に `.txt`/`.srt` があるファイルはスキップ。再生成は `--overwrite`。同じ名前で拡張子だけ違うファイル(`a.mp3` と `a.wav`)は出力名が重なり、後の方がスキップされる
- **長尺・大量件数**: 処理が長くなる場合は Bash の `run_in_background: true` で実行し、完了通知を待つ。途中経過は出力ファイルを Read で確認
- **出力先**: 入力と同じディレクトリ。別管理したい場合は生成後に mv

## 使用例

```bash
# 単一ファイル
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/path/clip.mp4"

# ディレクトリ配下のmp4をすべて(srtのみ)
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/videos" --formats srt

# 英語音声をCPUで
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/audio/talk.m4a" --lang en --device cpu
```
