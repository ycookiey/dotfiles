---
name: transcribe
description: "音声・動画ファイルをfaster-whisper(large-v3-turbo)で文字起こしし、txt/srtを出力する。GPU優先・CPUフォールバック。「文字起こし」「whisperで起こす」「動画/録画をテキストに」「transcribe」「字幕(srt)作成」等のリクエストで使用。mp4/mp3/wav/m4a/mkv/mov等に対応。"
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
- **冪等**: 既に `.txt`/`.srt` があるファイルはスキップ。再生成は `--overwrite`
- **長尺・大量件数**: 処理が長くなる場合は Bash の `run_in_background: true` で実行し、完了通知を待つ。途中経過は出力ファイルを Read で確認
- **出力先**: 入力と同じディレクトリ。別管理したい場合は生成後に mv

## 使用例

```bash
# 単一ファイル
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/path/lecture.mp4"

# ディレクトリ配下のmp4をすべて(srtのみ)
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/videos" --formats srt

# 英語音声をCPUで
uv run --python 3.12 --with faster-whisper --with nvidia-cublas-cu12 --with nvidia-cudnn-cu12 \
  "/c/Users/ycook/.claude-1/skills/transcribe/scripts/transcribe.py" "/c/audio/talk.m4a" --lang en --device cpu
```

## 確立の経緯
2026-05、TACT講義動画の文字起こしで GPU(RTX 5060) 実行を確立。
faster-whisper(CTranslate2) の GPU 実行に必要な `cublas64_12.dll` 等を pip wheel で供給し、
DLLディレクトリを登録することで CUDA Toolkit 無しに GPU 動作させている。
