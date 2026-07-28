# ローカル LLM 運用方針

前提ハードウェア: **VRAM 8GB (RTX 5060 / sm_120) と RAM 64GB**。
VRAM 量が違う環境では context 上限・`-ncmoe` 値が変わる。

## 使い分け (結論)

| 用途 | モデル | 速度 | context 上限 |
|---|---|---|---|
| **対話・コード・短文** | `Qwen3-8B-Q4_K_M` | **61.6 t/s** | 16K |
| **長文解析・大量 context・厳密な制約** | `Qwen3.6-35B-A3B-UD-Q4_K_M` (MoE) | 18.5〜23.1 t/s | **256K** |

判断基準:

- **即応性が要るなら Qwen3-8B**。MoE は同じ品質に到達するまで **実時間で 8〜67 倍**かかる
  (トークン消費が 3〜20 倍、速度が 2.7 倍遅いため)
- **16K を超える context / 複数制約の同時充足が要るなら MoE**
- **事実知識はどちらも捏造する**。外部検索を併用すること

## 別端末での再現

### 1. 推論エンジン

`install/scoopfile.json` に含まれる (bootstrap で自動導入):

```powershell
scoop install versions/llama.cpp-cu131
```

**`llama.cpp-cu124` は Blackwell/sm_120 で CUDA backend がロードされない** (CPU/RPC のみ)。
RTX 50 系では `cu131` を使う。

### 2. モデル取得

置き場所は任意 (例では `C:\Main\Tool\llm-models`)。**大きいので C:\ 直下や D:\ の可否は環境次第**。

```powershell
$d = "C:\Main\Tool\llm-models"; New-Item -ItemType Directory -Force $d | Out-Null

# 高速用 (4.68 GiB)
curl.exe -L --retry 10 --retry-all-errors -o "$d\Qwen3-8B-Q4_K_M.gguf" `
  "https://huggingface.co/Qwen/Qwen3-8B-GGUF/resolve/main/Qwen3-8B-Q4_K_M.gguf"

# 長 context・高品質用 (20.61 GiB)
curl.exe -L --retry 10 --retry-all-errors -o "$d\Qwen3.6-35B-A3B-UD-Q4_K_M.gguf" `
  "https://huggingface.co/unsloth/Qwen3.6-35B-A3B-GGUF/resolve/main/Qwen3.6-35B-A3B-UD-Q4_K_M.gguf"
```

DL 後に必ずサイズ照合する (途中切断が起きる):

```powershell
$f = [System.IO.File]::Open("$d\Qwen3-8B-Q4_K_M.gguf",'Open','Read','ReadWrite')
$f.Length; $f.Close()
(Invoke-WebRequest -Method Head $url -MaximumRedirection 5).Headers['Content-Length']
```

### 3. 起動

```powershell
# --- 高速・短 context ---
llama-server -m "$d\Qwen3-8B-Q4_K_M.gguf" `
  -ngl 99 -c 16384 -np 1 --port 8080 --jinja `
  --temp 0.6 --top-p 0.95 --top-k 20 --min-p 0

# --- 長 context・高品質 (MoE) ---
llama-server -m "$d\Qwen3.6-35B-A3B-UD-Q4_K_M.gguf" `
  -ngl 99 -ncmoe 99 -c 262144 -np 1 --port 8080 --jinja `
  --temp 0.6 --top-p 0.95 --top-k 20 --min-p 0
```

`-ncmoe N` = 先頭 N レイヤーの MoE expert を CPU RAM に置く。

| `-ncmoe` | 128K 時の VRAM | 速度 | 用途 |
|---|---|---|---|
| 36 | 7710 MiB | 19.3 t/s | 速度優先 (これ未満は OOM) |
| **99 (全部CPU)** | **6070 MiB** | 18.5 t/s | **context 優先。速度低下わずか 4%** |

sampler は **Qwen 公式推奨** (thinking mode): `temp 0.6 / top_p 0.95 / top_k 20 / min_p 0`。
llama.cpp 既定 (`temp 0.5 / top_p 0.85`) は推奨外だが、**正答率への影響は実測で確認できず**。

### OpenAI 互換 API

```powershell
$body = @{ model="local"; messages=@(@{role="user";content="..."}); max_tokens=7000 } |
        ConvertTo-Json -Depth 20 -Compress
Invoke-RestMethod -Uri "http://127.0.0.1:8080/v1/chat/completions" -Method Post `
  -Body ([System.Text.Encoding]::UTF8.GetBytes($body)) `
  -ContentType "application/json; charset=utf-8"
```

`tools` を渡せば function calling も動く (両モデルとも検証済み)。

## 罠

| 罠 | 対処 |
|---|---|
| **`-np` 未指定でスロット 4 分割** | `-c 131072` でも 1 req は 40960 しか使えない。**`-np 1` 必須** |
| **VRAM 超過が無言で RAM に退避** | NVIDIA の Windows ドライバの仕様。起動は成功するが速度が壊滅する。起動ログの `CUDA0 KV buffer size` と `model buffer size` の合計が VRAM 量を超えていないか確認する |
| **chat template 持ちモデルが対話 TUI に入る** | `llama-cli` は `-no-cnv` 非対応で詰み。非対話は **`llama-completion -no-cnv`** か `llama-bench` |
| **`Get-ChildItem` の Length が書き込み中は 0** | 大きいファイルの DL 進捗は `[System.IO.File]::Open(...).Length` で見る |
| **`run_in_background` はセッション終了で道連れ** | 長時間の DL は `Start-Process -WindowStyle Hidden -PassThru` で切り離す |
| **thinking モデルが max_tokens で打ち切られる** | コード生成で 7000 必要な場合あり。足りないと本体が空で返る |

## 検討したモデルと結果

| モデル | 量子化 | size | 速度 | context | 判定 |
|---|---|---|---|---|---|
| **Qwen3-8B** | Q4_K_M | 4.68 GiB | **61.6 t/s** | 16K | **採用 (高速用)** |
| **Qwen3.6-35B-A3B** | Q4_K_M | 20.61 GiB | 18.5〜23.1 t/s | **256K** | **採用 (高品質用)** |
| Qwen3.5-9B | Q4_K_M | 5.28 GiB | 55.7 t/s | — | 不採用。tool calling で並列呼び出しを落とし、日本語で発散 |
| Bonsai-27B | **Q1_0 (1bit)** | 3.53 GiB | 35.4 t/s | 32K | 不採用。**日本語 1/4** |
| Ternary-Bonsai-27B | Q2_0 (1.58bit) | 6.66 GiB | **4.2 t/s** | — | 不採用。VRAM には収まるがカーネルが未最適化で実用に耐えない |
| Bonsai-8B / 1.7B | Q1_0 | 1.07 / 0.23 GiB | 114.6 / 266.3 t/s | — | 不採用。推論が発散 (正答 2/5, 1/5) |

### 品質実測 (同一条件・同一問題)

| 領域 | Qwen3-8B Q4 | MoE 35B-A3B | Bonsai-27B (1bit) |
|---|---|---|---|
| tool calling (5) | 5/5 | **5/5** | 5/5 (引数が日本語になる等、質で劣る) |
| コーディング (3) | 3/3 | **3/3** | 2/3 |
| 日本語生成 (4) | 4/4 | **4/4** | **1/4** |
| 知識 (4) | 1/4 | **2/4** | — |
| 複数制約充足 | **✗ 字数違反と誤申告** | **✓** | — |
| 多段階推論 | ✓ | ✓ | — |
| 矛盾検出 | ✓ 3/3 | ✓ 3/3 | — |

### context 上限の実測 (VRAM 8151 MiB)

| モデル | 上限 | KV/token | 限界の理由 |
|---|---|---|---|
| Qwen3-8B Q4 | **16K** | 144 KiB | 24K で GPU要求 8216 MiB → RAM退避 |
| Bonsai-27B | 32K | 64 KiB | 64K で OOM |
| **MoE `-ncmoe 99`** | **256K** | **20 KiB** | 256K でも GPU要求 8002 MiB で収まる |

MoE が有利な理由は 2 つ:
1. `qwen35moe` は **hybrid attention (SSM 混成)** で KV が 1/7.2 に縮む
2. `-ncmoe 99` で GPU 側 model buffer が **1921 MiB** まで縮み、空きを全部 KV に回せる

## 1-bit 量子化 (Bonsai) について

`prism-ml/Bonsai-*` は GGUF **Q1_0** (1.125 bit/weight)。**upstream llama.cpp では読めない**
(`invalid ggml type 41`)。[PrismML fork](https://github.com/PrismML-Eng/llama.cpp) が必須で、
Windows は **CUDA 12.4 と Vulkan** の prebuilt のみ。sm_120 では **Vulkan 版が正常動作**する
(`matrix cores: NV_coopmat2` を認識)。

**採用しなかった**。「8GB に 27B を載せる」目的自体は MoE がより高品質・
より長い context で達成しており、1bit 化の代償 (日本語の崩壊、32K context 上限)
に見合わない。

## 推論エンジンの選択肢

llama.cpp 以外は **この構成では実質不可**:

- **vLLM**: Windows 公式サポートなし。RTX 50 系 prebuilt が動かない。CPU オフロードが弱く 20GB は載らない
- **ik_llama.cpp**: MoE と CPU オフロードの組み合わせで mainline より遅いという報告
- **KTransformers**: 高速化が **AMX 前提**。Zen 2 (Ryzen 5 3600) は非搭載
- **ExLlamaV3**: VRAM 内に収まるモデル専用。MoE オフロード非対応

backend は用途で使い分ける (※ fork は b9596 系、cu131 は b8140 でバージョン差が交絡):

| | Vulkan (PrismML fork) | CUDA (本家 cu131) |
|---|---|---|
| MoE tg | 19.3 t/s | **23.1 t/s** |
| MoE pp | 45.2 t/s | **93.7 t/s** |
| dense 9B tg | **55.7 t/s** | 44.9 t/s |
