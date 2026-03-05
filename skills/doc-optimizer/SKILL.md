---
name: doc-optimizer
description: Optimize docs for AI readability & token efficiency (High-Density Japanese).
allowed-tools: Read, Replace, Write
---

# Document Optimizer Skill

Convert docs to "High-Density Japanese" format for max token efficiency while keeping info.

## Optimization Rules

### 1. Terminology (Mix JP & EN)
Keep Japanese grammar structure. Replace **Keywords Only**.
- **Replace:** Katakana terms -> English (e.g., プロジェクト -> Project).
- **Keep:** Particles (minimal), Conjunctions, Nuance in Japanese.
- **Goal:** "High-Density Japanese" (Not English).

### 2. Syntax & Style
- Lists: `-` (Hyphen).
- Brackets: Half-width `()`.
- Sentences:
  - Endings: Remove "です/ます". Use "〜する/〜である" or Noun stop.
  - **Do Not** translate full sentences to English.

### 3. Structure Example
Convert redundant text to structured data.

**Before:**
> このディレクトリには、過去のアウトプット（登壇資料や記事など）を格納します。これによってAIに自分の思考パターンを学習させる効果があります。

**After:**
- `publications/`: 過去Output(登壇資料, 記事)。AIに思考Patternを学習させる。

## Process Flow

1. Analyze:
   - Understand file structure & intent.
   - Identify Core Value.

2. Draft:
   - Create optimized text per rule.
   - Check: Ensure no missing Facts.

3. Execute:
   - Update file via `Replace` or `Write`.

## Usage

**User:** "README.mdを最適化して"
**Agent:** Read `README.md` -> Optimize -> Update.