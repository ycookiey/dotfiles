#!/usr/bin/env node
// Claude Codeセッションのcwdを別パスに移行する
// Usage: claude-session-migrate <from-path> [to-path]
//   from-pathが .claude/worktrees/* 配下の場合、to-pathは自動推論可能

const fs = require('fs');
const path = require('path');

const CLAUDE_DIR = path.join(process.env.HOME || process.env.USERPROFILE, '.claude');
const PROJECTS_DIR = path.join(CLAUDE_DIR, 'projects');

function usage() {
  console.error('Usage: claude-session-migrate <from-path> [to-path]');
  console.error('  from-path: 移行元のcwd (例: /c/Main/Project/Foo/.claude/worktrees/bar)');
  console.error('  to-path:   移行先のcwd (worktreeの場合は省略可)');
  process.exit(1);
}

// パスをWindows形式に正規化 (C:\Main\... 形式)
function toWindowsPath(p) {
  p = path.resolve(p);
  // /c/... → C:\...
  if (/^\/[a-zA-Z]\//.test(p)) {
    p = p[1].toUpperCase() + ':' + p.slice(2);
  }
  return p.replace(/\//g, '\\');
}

// Windowsパス → projects/ディレクトリ名 (C:\foo\bar → C--foo--bar)
function toProjectDirName(winPath) {
  return winPath.replace(/[:\\]/g, (ch) => ch === ':' ? '-' : '-');
}

// .claude/worktrees/* パスからmainプロジェクトパスを推論
function inferMainProject(winPath) {
  const marker = '\\.claude\\worktrees\\';
  const idx = winPath.indexOf(marker);
  if (idx === -1) return null;
  return winPath.substring(0, idx);
}

function main() {
  const args = process.argv.slice(2);
  if (args.length === 0 || args.includes('--help') || args.includes('-h')) {
    usage();
  }

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

  if (!fs.existsSync(fromDir)) {
    console.error(`Error: 移行元が存在しない: ${fromDir}`);
    process.exit(1);
  }
  if (!fs.existsSync(toDir)) {
    console.error(`Error: 移行先が存在しない: ${toDir}`);
    console.error('先に移行先ディレクトリでClaude Codeを一度起動してください');
    process.exit(1);
  }

  // jsonlファイルを列挙
  const jsonlFiles = fs.readdirSync(fromDir).filter(f => f.endsWith('.jsonl'));
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
