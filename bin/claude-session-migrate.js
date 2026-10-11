#!/usr/bin/env node
// Claude Codeセッションのcwdを別パスに移行する
// Usage: claude-session-migrate [--dry-run] <from-path> [to-path]
//   from-pathが .claude/worktrees/* 配下の場合、to-pathは自動推論可能

const fs = require('fs');
const path = require('path');

// Claude Code は CLAUDE_CONFIG_DIR があればそれを設定ディレクトリにする
const CLAUDE_DIR = process.env.CLAUDE_CONFIG_DIR
  || path.join(process.env.HOME || process.env.USERPROFILE, '.claude');
const PROJECTS_DIR = path.join(CLAUDE_DIR, 'projects');

// Claude Code は長いディレクトリ名を切り詰めて hash を付けることがある (規則は未確認)。
// 生成名で見つからない時、この長さまでの前方一致で候補を出す
const CANDIDATE_PREFIX_LEN = 200;

function usage() {
  console.error('Usage: claude-session-migrate [--dry-run] <from-path> [to-path]');
  console.error('  from-path: 移行元のcwd (例: /c/Main/Project/Foo/.claude/worktrees/bar)');
  console.error('  to-path:   移行先のcwd (worktreeの場合は省略可)');
  console.error('  --dry-run: 解決した移行元/移行先ディレクトリを表示して終了 (ファイルを変更しない)');
  process.exit(1);
}

// パスをWindows形式に正規化 (C:\Main\... 形式)
function toWindowsPath(p) {
  // /c/... → C:/... を path.resolve より先に行う (win32 の resolve は /c/x を C:\c\x にする)
  if (/^\/[a-zA-Z](\/|$)/.test(p)) {
    p = p[1] + ':' + (p.slice(2) || '/');
  }
  p = path.resolve(p).replace(/\//g, '\\');
  // jsonl 内の cwd は大文字ドライブなので、c:\... で渡されても置換が一致するよう揃える
  return p.replace(/^[a-z]:/, (d) => d.toUpperCase());
}

// Windowsパス → projects/ディレクトリ名 (Claude Code は英数字以外をすべて - にする)
// 例: C:\Foo\.claude\worktrees\bar → C--Foo--claude-worktrees-bar
function toProjectDirName(winPath) {
  return winPath.replace(/[^a-zA-Z0-9]/g, '-');
}

// 生成名で見つからない時の候補 (projects 内で前方一致するディレクトリ)
function findCandidates(dirName) {
  const prefix = dirName.slice(0, CANDIDATE_PREFIX_LEN);
  try {
    return fs.readdirSync(PROJECTS_DIR).filter((d) => d !== dirName && d.startsWith(prefix));
  } catch {
    return [];
  }
}

function reportMissing(label, dir, dirName) {
  console.error(`Error: ${label}が存在しない: ${dir}`);
  const candidates = findCandidates(dirName);
  if (candidates.length > 0) {
    console.error(`  projects 内の前方一致候補 (${PROJECTS_DIR}):`);
    for (const c of candidates) console.error(`    ${c}`);
  }
}

// .claude/worktrees/* パスからmainプロジェクトパスを推論
function inferMainProject(winPath) {
  const marker = '\\.claude\\worktrees\\';
  const idx = winPath.indexOf(marker);
  if (idx === -1) return null;
  return winPath.substring(0, idx);
}

function main() {
  const argv = process.argv.slice(2);
  if (argv.includes('--help') || argv.includes('-h')) usage();
  const dryRun = argv.includes('--dry-run');
  const unknown = argv.filter((a) => a.startsWith('--') && a !== '--dry-run');
  if (unknown.length > 0) {
    console.error(`Error: 不明なオプション: ${unknown.join(' ')}`);
    usage();
  }
  const args = argv.filter((a) => !a.startsWith('--'));
  if (args.length === 0 || args.length > 2) usage();

  const fromPath = toWindowsPath(args[0]);
  let toPath;

  if (args.length >= 2) {
    toPath = toWindowsPath(args[1]);
  } else {
    toPath = inferMainProject(fromPath);
    if (!toPath) {
      console.error('Error: to-path を省略できるのは from-path が .claude/worktrees/* の場合のみ');
      usage();
    }
    console.log(`to-path を自動推論: ${toPath}`);
  }

  const fromDirName = toProjectDirName(fromPath);
  const toDirName = toProjectDirName(toPath);
  const fromDir = path.join(PROJECTS_DIR, fromDirName);
  const toDir = path.join(PROJECTS_DIR, toDirName);

  console.log(`移行元: ${fromDir}`);
  console.log(`移行先: ${toDir}`);

  if (!fs.existsSync(fromDir)) {
    reportMissing('移行元', fromDir, fromDirName);
    process.exit(1);
  }
  if (!fs.existsSync(toDir)) {
    reportMissing('移行先', toDir, toDirName);
    console.error('先に移行先ディレクトリでClaude Codeを一度起動してください');
    process.exit(1);
  }

  // jsonlファイルを列挙
  const jsonlFiles = fs.readdirSync(fromDir).filter(f => f.endsWith('.jsonl'));
  if (dryRun) {
    const clashes = jsonlFiles.filter((f) => fs.existsSync(path.join(toDir, f)));
    console.log(`dry-run: ${jsonlFiles.length} セッションが移行対象` +
      (clashes.length ? ` (うち ${clashes.length} 件は移行先に同名があり SKIP)` : ''));
    process.exit(0);
  }
  if (jsonlFiles.length === 0) {
    console.log('移行対象のセッションなし');
    process.exit(0);
  }

  console.log(`${jsonlFiles.length} セッションを移行: ${fromDirName} → ${toDirName}`);

  // jsonl内のcwd置換用文字列 (ファイル内はJSON形式でバックスラッシュがエスケープされている)
  const fromCwdEscaped = fromPath.replace(/\\/g, '\\\\');
  const toCwdEscaped = toPath.replace(/\\/g, '\\\\');

  let migrated = 0;
  for (const file of jsonlFiles) {
    const srcPath = path.join(fromDir, file);
    const destPath = path.join(toDir, file);

    if (fs.existsSync(destPath)) {
      console.log(`  SKIP: ${file} (移行先に同名ファイルが存在)`);
      continue;
    }

    let content = fs.readFileSync(srcPath, 'utf8');

    // worktree-state行を除去
    const lines = content.split('\n');
    const filtered = lines.filter(line => {
      if (!line.trim()) return true;
      try {
        const obj = JSON.parse(line);
        return obj.type !== 'worktree-state';
      } catch {
        return true;
      }
    });
    content = filtered.join('\n');

    // cwd置換
    content = content.replaceAll(fromCwdEscaped, toCwdEscaped);

    fs.writeFileSync(destPath, content);
    fs.unlinkSync(srcPath);
    migrated++;
    console.log(`  OK: ${file}`);
  }

  // サブエージェントディレクトリは移動しない (不要)
  // 空になったfromDirの残存ファイル確認
  const remaining = fs.readdirSync(fromDir);
  if (remaining.length === 0) {
    fs.rmdirSync(fromDir);
    console.log(`移行元ディレクトリを削除: ${fromDirName}`);
  } else {
    const dirs = remaining.filter(f => fs.statSync(path.join(fromDir, f)).isDirectory());
    const files = remaining.filter(f => !fs.statSync(path.join(fromDir, f)).isDirectory());
    if (files.length === 0 && dirs.length > 0) {
      // サブエージェントディレクトリのみ残存 → 削除
      for (const d of dirs) {
        fs.rmSync(path.join(fromDir, d), { recursive: true });
      }
      fs.rmdirSync(fromDir);
      console.log(`移行元ディレクトリを削除: ${fromDirName} (サブエージェントログ含む)`);
    } else {
      console.log(`移行元に ${remaining.length} 件残存: ${fromDirName}`);
    }
  }

  console.log(`完了: ${migrated} セッション移行`);
}

main();
