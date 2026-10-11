# YouTube 動画の内容把握(文字起こし取得)

動画を視聴せずに内容を把握したい時の手順。WebFetch は YouTube のフッターしか返さない。

## 動かない方法(2026-09 時点)

| 方法 | 結果 |
|---|---|
| `WebFetch(watch URL)` | タイトル以外ほぼ空(JS レンダリング前のフッターのみ) |
| Chrome で「文字起こしを表示」を JS クリック | パネルは EXPANDED になるが `ytd-transcript-segment-renderer` が 0 件のまま(遅延ロード・DOM 構造変動) |
| `ytInitialPlayerResponse.captions...baseUrl` を fetch | 空レスポンス → JSON parse error(`pot` トークン必須化) |

## 動く方法

### 1. メタデータ(タイトル・チャンネル)は oEmbed

```bash
curl -s "https://www.youtube.com/oembed?url=https://www.youtube.com/watch?v=<ID>&format=json" | jq '{title,author_name}'
# プレイリストも同じ形で可: url=https://www.youtube.com/playlist?list=<LIST>
```

### 2. 本文は youtube-transcript-api(uv で一発)

```bash
uv run --python 3.12 --with youtube-transcript-api python - <<'EOF'
from youtube_transcript_api import YouTubeTranscriptApi
api = YouTubeTranscriptApi()
tl = api.list("<VIDEO_ID>")
print([(t.language_code, t.is_generated) for t in tl])   # 利用可能トラック
t = tl.find_transcript(["ja", "en"]).fetch()              # 手動字幕優先
print(" ".join(s.text.replace("\n", " ") for s in t))
EOF
```

- 自動生成(`is_generated=True`)しかない英語動画でも精度は実用レベル
- `s.start` を使えばタイムスタンプ付きで章立て抽出できる(`re.search("By the end of this video|Let's talk about", ...)` 等で総集編のエピソード境界を拾う)
- 55 分動画で約 5 万字。要約前に全文を取ってから grep する

### 3. プレイリストの収録一覧は Chrome の DOM から

`ytd-playlist-video-renderer` は新 UI で空になる。`a[href*="watch?v="][title]` / `a#video-title` を集めて `v=` で重複除去する。

## 注意

- 字幕取得は API 呼び出しであってファイルダウンロードではない(ブラウザ操作ルールの「ダウンロード許可」対象外)
- 再生を伴わないので、PC の音声出力を使う他の処理とは干渉しない
