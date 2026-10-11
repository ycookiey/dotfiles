-- セッション復元: 窓・タブ・分割ペインの配置と各ペインの cwd を定期保存し、
-- 前回の WezTerm プロセスの状態をキー操作で復元する。
-- Claude Code が動いていたペインは、その会話 ID を再開するコマンドで開く。
--
-- 保存先: %USERPROFILE%\.local\share\wezterm\session-snapshot-<instance>.json
--   (WezTerm プロセスごとに 1 ファイル。新しい起動が前回分を上書きしない)
-- 会話 ID の取得元 (優先順):
--   1. %USERPROFILE%\.local\state\claude-pid-sessions\<pid>.json
--      (Claude Code の SessionStart hook が CLAUDE_PID ごとに stdin JSON を保存。/clear に追従)
--   2. %USERPROFILE%\.claude\sessions\<pid>.json (Claude Code 本体の起動中セッション一覧)
local wezterm = require("wezterm")

local M = {}

local home = os.getenv("USERPROFILE") or os.getenv("HOME") or ""
local snapshot_dir = home .. "\\.local\\share\\wezterm"
local snapshot_prefix = "session-snapshot-"
local record_dir = home .. "\\.local\\state\\claude-pid-sessions"
local registry_dir = home .. "\\.claude\\sessions"

local SAVE_INTERVAL = 30 -- 秒
local PRUNE_EVERY = 20 -- 保存 n 回ごとに古いファイルを掃除
local KEEP_SNAPSHOTS = 10
local RESUME_STAGGER_MS = 1000 -- Claude を同時起動しないよう 1 ペインごとにずらす

local opts = {
  -- 復元する各ペインの起動コマンド (argv) を作る。nil なら default_prog で開く。
  -- cwd: そのペインのフォルダ。spawn / split に渡した cwd が無視され、分割元などのフォルダで
  --   開くことがあった (実測。起動コマンドの有無にかかわらず) ため、コマンド側でも移動するとよい
  -- session_id: Claude Code の会話を再開するペインならその ID、それ以外は nil
  -- delay_ms: 複数の Claude が同時に起動しないよう、ペインごとにずらす待ち時間の目安
  pane_args = function(cwd, session_id, delay_ms)
    if session_id then return { "claude", "--resume", session_id } end
    return nil
  end,
  -- 再開できなかったペインのシェルに、やり直しとして打ち込むコマンド
  retry_command = function(session_id)
    return "claude --resume " .. session_id
  end,
}

local function is_uuid(s)
  return type(s) == "string"
    and s:match("^%x%x%x%x%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%-%x%x%x%x%x%x%x%x%x%x%x%x$") ~= nil
end

local function file_exists(path)
  local f = io.open(path, "rb")
  if f then
    f:close()
    return true
  end
  return false
end

local function read_json(path)
  local f = io.open(path, "rb")
  if not f then return nil end
  local s = f:read("*a")
  f:close()
  local ok, v = pcall(wezterm.serde.json_decode, s)
  if ok and type(v) == "table" then return v end
  return nil
end

-- Windows の os.rename は既存の宛先を上書きできないため、tmp → 削除 → rename の順で置き換える
local function write_json(path, value)
  local tmp = path .. ".tmp"
  local f = io.open(tmp, "wb")
  if not f then return false end
  f:write(wezterm.serde.json_encode(value))
  f:close()
  os.remove(path)
  return os.rename(tmp, path) ~= nil
end

local function proc_name(info)
  return ((info and info.name) or ""):lower()
end

-- Claude Code の自動更新は、実行中の claude.exe を claude.exe.old.<時刻>.<pid> に改名する (実測)
local function is_claude_name(name)
  return name == "claude" or name:match("^claude%.exe") ~= nil
end

local function current_pid()
  return wezterm.procinfo.pid()
end

local function instance_id()
  if not wezterm.GLOBAL.session_restore_instance then
    wezterm.GLOBAL.session_restore_instance = string.format("%d-%d", os.time(), current_pid())
  end
  return wezterm.GLOBAL.session_restore_instance
end

local function cwd_of(pane)
  local ok, url = pcall(function() return pane:get_current_working_dir() end)
  if not ok or not url then return nil end
  local path = type(url) == "string" and url:gsub("^file://[^/]*", "") or url.file_path
  if not path then return nil end
  -- /C:/foo/ → C:/foo/
  return (path:gsub("^/(%a):", "%1:"))
end

-- ペインの前面プロセスから親をたどり、シェルに最も近い claude を返す。
-- Claude から起動された入れ子の `claude -p` はこれで除外される
local function top_claude_pid(pane)
  local ok, fg = pcall(function() return pane:get_foreground_process_info() end)
  if not ok or not fg then return nil end
  -- 前面のプロセスは短命なもの (Claude のステータス行の表示で起動するコマンド等) のことがあり、
  -- pid で問い合わせ直すと既に終了していて nil になる (実測)。最初は受け取った情報をそのまま使う
  local pid, info, top = fg.pid, fg, nil
  local gui_pid = current_pid()
  for _ = 1, 32 do
    local name = proc_name(info)
    if is_claude_name(name) then top = pid end
    -- 親がこの GUI なら問い合わせずに止める。get_info_for_pid は渡した pid の配下の全プロセス木を
    -- キャッシュせずに作るため、GUI の pid を渡すと全ペインのプロセスを読むことになる。
    -- 名前の判定の後に置くので、GUI の直下で動く claude (`wezterm start -- claude`) も拾う
    if info.ppid == gui_pid then break end
    if name:match("^wezterm") or not info.ppid or info.ppid == 0 or info.ppid == pid then break end
    pid = info.ppid
    info = wezterm.procinfo.get_info_for_pid(pid)
    if not info then break end
  end
  if top then return top end
  -- WezTerm の前面のプロセスの情報は古いことがある (出力の無い間に起動した claude が見えず、
  -- シェルのままだった。実測)。問い合わせ直して子孫を浅い順に探す
  local fresh = wezterm.procinfo.get_info_for_pid(fg.pid)
  local queue = { fresh }
  while #queue > 0 do
    local node = table.remove(queue, 1)
    for cpid, child in pairs((node and node.children) or {}) do
      if is_claude_name(proc_name(child)) then return child.pid or cpid end
      table.insert(queue, child)
    end
  end
  return nil
end

local function session_for_pid(pid)
  local rec = read_json(record_dir .. "\\" .. pid .. ".json")
  if rec and is_uuid(rec.session_id) then
    return { id = rec.session_id, transcript = rec.transcript_path, cwd = rec.cwd }
  end
  local reg = read_json(registry_dir .. "\\" .. pid .. ".json")
  if reg and is_uuid(reg.sessionId) and (reg.kind == nil or reg.kind == "interactive") then
    return { id = reg.sessionId, cwd = reg.cwd }
  end
  return nil
end

-- 最大化を直接返す API が無いため、窓の大きさがどれかのモニターとほぼ同じかで判定する
local function looks_maximized(dims)
  local ok, screens = pcall(wezterm.gui.screens)
  if not ok or not screens then return false end
  for _, sc in pairs(screens.by_name or {}) do
    if math.abs(dims.pixel_width - sc.width) <= 32
      and dims.pixel_height >= sc.height * 0.85 and dims.pixel_height <= sc.height + 32 then
      return true
    end
  end
  return false
end

-- 窓の大きさ。cols / rows はタブの領域のセル数で、復元後に合っているかの判定に使う
local function window_size(mw)
  local tab = mw:active_tab()
  local size = tab and tab:get_size()
  local gui = mw:gui_window()
  local dims = gui and gui:get_dimensions()
  if not size or not dims then return nil end
  return {
    cols = size.cols,
    rows = size.rows,
    pixel_width = dims.pixel_width,
    pixel_height = dims.pixel_height,
    maximized = looks_maximized(dims),
  }
end

local function snapshot()
  local windows = {}
  local pane_count = 0
  for _, mw in ipairs(wezterm.mux.all_windows()) do
    local active_tab = mw:active_tab()
    local tabs = {}
    for _, tab in ipairs(mw:tabs()) do
      local panes = {}
      -- ズーム中でも panes_with_info は全ペインを元の配置で返す
      for _, info in ipairs(tab:panes_with_info()) do
        local p = {
          left = info.left,
          top = info.top,
          width = info.width,
          height = info.height,
          active = info.is_active,
          cwd = cwd_of(info.pane),
        }
        local cpid = top_claude_pid(info.pane)
        if cpid then p.claude = session_for_pid(cpid) end
        table.insert(panes, p)
        pane_count = pane_count + 1
      end
      table.insert(tabs, {
        title = tab:get_title(),
        active = active_tab ~= nil and active_tab:tab_id() == tab:tab_id(),
        panes = panes,
      })
    end
    table.insert(windows, { size = window_size(mw), tabs = tabs })
  end
  return {
    version = 1,
    instance = instance_id(),
    gui_pid = current_pid(),
    saved_at = os.time(),
    pane_count = pane_count,
    windows = windows,
  }
end

local function list_snapshot_files()
  local ok, entries = pcall(wezterm.read_dir, snapshot_dir)
  if not ok then return {} end
  local files = {}
  for _, path in ipairs(entries) do
    local name = path:match("([^/\\]+)$") or ""
    if name:sub(1, #snapshot_prefix) == snapshot_prefix and name:match("%.json$") then
      table.insert(files, path)
    end
  end
  table.sort(files) -- ファイル名の instance が起動時刻で始まるため、古い順になる
  return files
end

local function prune()
  local files = list_snapshot_files()
  for i = 1, #files - KEEP_SNAPSHOTS do
    os.remove(files[i])
  end
  local ok, entries = pcall(wezterm.read_dir, record_dir)
  if not ok then return end
  for _, path in ipairs(entries) do
    local pid = tonumber((path:match("([^/\\]+)$") or ""):match("^(%d+)%.json$"))
    if pid and not is_claude_name(proc_name(wezterm.procinfo.get_info_for_pid(pid))) then
      os.remove(path)
    end
  end
end

local function save()
  local snap = snapshot()
  write_json(snapshot_dir .. "\\" .. snapshot_prefix .. snap.instance .. ".json", snap)
end

-- この Lua の状態で保存ループを始めたか。
-- 設定の評価時に call_after でループを始めると、読込ごとに 2 回評価され、後の評価で仕掛けた
-- ループが一度も発火しなかった (実測。原因は未確認)。そのため評価時ではなく
-- window-config-reloaded のイベントからループを始める
local loop_started = false

local function start_saving()
  local gen = (wezterm.GLOBAL.session_restore_gen or 0) + 1
  wezterm.GLOBAL.session_restore_gen = gen
  local ticks = 0
  local function tick()
    -- 設定の再読み込みで新しいループが始まったら、古いループは止まる
    if wezterm.GLOBAL.session_restore_gen ~= gen then return end
    local ok, err = pcall(function()
      -- 読込の重なりでループが複数走っても、書き込みは間隔ごとに 1 回にする
      local now = os.time()
      if now - (wezterm.GLOBAL.session_restore_last_save or 0) < SAVE_INTERVAL - 1 then return end
      wezterm.GLOBAL.session_restore_last_save = now
      save()
      ticks = ticks + 1
      if ticks % PRUNE_EVERY == 1 then prune() end
    end)
    if not ok then wezterm.log_error("session_restore: save failed: " .. tostring(err)) end
    wezterm.time.call_after(SAVE_INTERVAL, tick)
  end
  wezterm.time.call_after(5, tick)
end

-- 前回の WezTerm プロセス (すでに終了したもの) の保存を選ぶ。
-- 再起動では全プロセスがほぼ同時に終わるため、最後に終わった時刻の付近で終わったもののうち
-- ペインが最も多いものを選ぶ (`wezterm start -- <prog>` で開いた単機能の窓を避ける)
local function pick_previous()
  local me, my_pid = instance_id(), current_pid()
  local dead, newest = {}, 0
  for _, path in ipairs(list_snapshot_files()) do
    local snap = read_json(path) or read_json(path .. ".tmp")
    if snap and snap.instance ~= me and (snap.pane_count or 0) > 0 then
      -- 動いている WezTerm は保存を更新し続ける。古い保存は、pid が別のプロセスに再利用されていても終了済みとみなす
      local alive = false
      if snap.gui_pid ~= my_pid and os.time() - (snap.saved_at or 0) <= 2 * SAVE_INTERVAL + 30 then
        alive = proc_name(wezterm.procinfo.get_info_for_pid(snap.gui_pid)):match("^wezterm%-gui") ~= nil
      end
      if not alive then
        table.insert(dead, snap)
        newest = math.max(newest, snap.saved_at)
      end
    end
  end
  local best
  for _, snap in ipairs(dead) do
    if snap.saved_at >= newest - (2 * SAVE_INTERVAL + 30) then
      if not best or snap.pane_count > best.pane_count
        or (snap.pane_count == best.pane_count and snap.saved_at > best.saved_at) then
        best = snap
      end
    end
  end
  return best
end

-- ペインの矩形の集まりを、端から端まで通る切れ目で再帰的に二分し、分割の木に戻す
local function extent(panes, pos, size)
  local lo, hi = math.huge, -math.huge
  for _, p in ipairs(panes) do
    lo = math.min(lo, p[pos])
    hi = math.max(hi, p[pos] + p[size])
  end
  return lo, hi
end

local function try_cut(panes, pos, size, direction)
  local cuts = {}
  for _, p in ipairs(panes) do table.insert(cuts, p[pos] + p[size]) end
  table.sort(cuts)
  for _, cut in ipairs(cuts) do
    local first, second = {}, {}
    for _, p in ipairs(panes) do
      if p[pos] + p[size] <= cut then
        table.insert(first, p)
      elseif p[pos] > cut then
        table.insert(second, p)
      end
    end
    if #first > 0 and #second > 0 and #first + #second == #panes then
      local lo1, hi1 = extent(first, pos, size)
      local lo2, hi2 = extent(second, pos, size)
      local a, b = hi1 - lo1, hi2 - lo2
      return { direction = direction, first = first, second = second, ratio = b / (a + b) }
    end
  end
  return nil
end

local function build_tree(panes)
  if #panes == 1 then return { leaf = panes[1] } end
  local cut = try_cut(panes, "left", "width", "Right") or try_cut(panes, "top", "height", "Bottom")
  if not cut then
    -- 通常は起きない (WezTerm の分割は常に二分木)。崩れた保存でも全ペインは開く
    local rest = {}
    for i = 2, #panes do table.insert(rest, panes[i]) end
    cut = { direction = "Right", first = { panes[1] }, second = rest, ratio = (#panes - 1) / #panes }
  end
  return {
    direction = cut.direction,
    ratio = cut.ratio,
    first = build_tree(cut.first),
    second = build_tree(cut.second),
  }
end

local function first_leaf(node)
  while not node.leaf do node = node.first end
  return node.leaf
end

local function spawn_opts(leaf)
  local o = {}
  if leaf.cwd then o.cwd = leaf.cwd end
  if leaf.spawn_args then o.args = leaf.spawn_args end
  return o
end

-- 会話の記録ファイル (<ID>.jsonl) があるか。
-- hook が受け取る transcript_path は起動時の作業フォルダから組み立てたもので、会話を始めた
-- フォルダと違うと実在しない (実測) ため、見つからなければ各 projects フォルダを探す
local function transcript_exists(c, projects_dirs)
  if c.transcript and file_exists(c.transcript) then return true end
  for _, dir in ipairs(projects_dirs) do
    if file_exists(dir .. "\\" .. c.id .. ".jsonl") then return true end
  end
  return false
end

-- ~/.claude*/projects/* (Claude Code の設定フォルダごとの会話の置き場)。
-- 既定の ~/.claude を先に並べる (見つかった時点で探すのをやめるため)
local function list_projects_dirs()
  local dirs = {}
  local ok, entries = pcall(wezterm.read_dir, home)
  if not ok then return dirs end
  table.sort(entries, function(a, b)
    local da = (a:match("([^/\\]+)$") or "") == ".claude"
    local db = (b:match("([^/\\]+)$") or "") == ".claude"
    if da ~= db then return da end
    return a < b
  end)
  for _, entry in ipairs(entries) do
    local name = entry:match("([^/\\]+)$") or ""
    if name:match("^%.claude") then
      local ok2, projects = pcall(wezterm.read_dir, entry .. "\\projects")
      if ok2 then
        for _, dir in ipairs(projects) do table.insert(dirs, dir) end
      end
    end
  end
  return dirs
end

-- 各ペインに起動コマンドを割り当て、会話を再開する数を返す。
-- 後からシェルへ文字を送る方式 (call_after + send_text) ではコマンドが実行されなかったため
-- (実測。原因は未確認)、ペインの起動コマンド自体に含める
local function assign_spawn_args(snap)
  local n = 0
  local projects_dirs = list_projects_dirs()
  for _, w in ipairs(snap.windows or {}) do
    for _, t in ipairs(w.tabs or {}) do
      for _, p in ipairs(t.panes or {}) do
        local c = p.claude
        -- /clear 直後で会話の中身がまだ無いものは、シェルだけ開く
        p.resumable = c ~= nil and is_uuid(c.id) and transcript_exists(c, projects_dirs)
        if p.resumable then
          p.spawn_args = opts.pane_args(p.cwd, c.id, n * RESUME_STAGGER_MS)
          n = n + 1
        else
          p.spawn_args = opts.pane_args(p.cwd, nil, 0)
        end
      end
    end
  end
  return n
end

-- pane は node の最初の葉のペイン。木を外側から分割していく
local function realize(node, pane, out)
  if node.leaf then
    table.insert(out, { pane = pane, spec = node.leaf })
    return
  end
  local args = spawn_opts(first_leaf(node.second))
  args.direction = node.direction
  args.size = node.ratio
  local new_pane = pane:split(args)
  realize(node.first, pane, out)
  realize(node.second, new_pane, out)
end

local function restore_tab(tab_spec, spawn)
  local tree = build_tree(tab_spec.panes)
  local tab, root = spawn(spawn_opts(first_leaf(tree)))
  local placed = {}
  realize(tree, root, placed)
  if tab_spec.title and tab_spec.title ~= "" then tab:set_title(tab_spec.title) end
  return tab, placed
end

-- 復元で新しく開いた窓のうち、GUI の窓ができたら最大化するもの (mux の window_id の集合)。
-- window-config-reloaded だけでは取りこぼす窓があった (実測) ため、update-status でも確かめる
local maximize_on_open = {}

-- 復元後の確認。spawn / split の指定 (cwd・起動コマンド) が丸ごと使われないことがあった (実測、
-- 9 ペイン中 1 つ、原因は未確認) ため、会話を再開するはずのペインで claude が動いているかを
-- 少し後に確かめる。段階:
--   0: 見つからなければ 10 秒後にもう一度確かめる (判定の取りこぼしで Claude に文字を送らないため)
--   1: それでも見つからなければ、そのペインのシェルに再開コマンドを打ち込み、20 秒後に確かめる
--   2: まだ見つからなければ、手で戻すコマンドを通知とファイルで知らせる
-- 文字を送るのは、ペインを作ってから 30 秒以上たち、シェルが入力待ちになっている時点だけ
local verify_list, verify_at, verify_stage = nil, nil, 0
local failed_file = snapshot_dir .. "\\session-restore-failed.txt"

local function verify_resumed(window)
  if not verify_at or os.time() < verify_at then return end
  local list = verify_list
  verify_list, verify_at = nil, nil
  local missing = {}
  for _, item in ipairs(list or {}) do
    local pane = wezterm.mux.get_pane(item.pane_id)
    if pane and not top_claude_pid(pane) then table.insert(missing, item) end
  end
  if #missing == 0 then return end
  if verify_stage == 0 then
    verify_stage = 1
    verify_list, verify_at = missing, os.time() + 10
    return
  end
  if verify_stage == 1 then
    verify_stage = 2
    for _, item in ipairs(missing) do
      local pane = wezterm.mux.get_pane(item.pane_id)
      local text = opts.retry_command(item.session_id)
      wezterm.log_info("session_restore: retry pane " .. item.pane_id .. ": " .. text)
      pane:send_text(text .. "\r")
    end
    verify_list, verify_at = missing, os.time() + 20
    return
  end
  local lines = {}
  for _, item in ipairs(missing) do
    table.insert(lines, string.format("%s  %s", item.cwd or "?", opts.retry_command(item.session_id)))
  end
  local f = io.open(failed_file, "wb")
  if f then
    f:write(table.concat(lines, "\n") .. "\n")
    f:close()
  end
  wezterm.log_error("session_restore: not resumed: " .. table.concat(lines, " | "))
  window:toast_notification(
    "WezTerm",
    string.format("%d 個の会話を再開できなかった。戻すコマンド: %s", #missing, failed_file),
    nil,
    15000
  )
end

local function build(window, pane, snap)
  local done_key = "session_restore_done_" .. snap.instance
  if wezterm.GLOBAL[done_key] then
    window:toast_notification("WezTerm", "前回の状態はこの起動中に復元済み", nil, 4000)
    return
  end
  wezterm.GLOBAL[done_key] = true

  local mux_window = window:mux_window()
  -- 起動直後の 1 タブ 1 ペインだけの窓なら、復元後にその初期ペインを閉じる
  local fresh = #mux_window:tabs() == 1 and #mux_window:tabs()[1]:panes() == 1
  local resumed = assign_spawn_args(snap)
  local all_placed, actives = {}, {}

  for wi, win_spec in ipairs(snap.windows or {}) do
    local target = wi == 1 and mux_window or nil
    local active_tab
    for _, tab_spec in ipairs(win_spec.tabs or {}) do
      if #(tab_spec.panes or {}) > 0 then
        local tab, placed = restore_tab(tab_spec, function(o)
          if target then
            local t, p = target:spawn_tab(o)
            return t, p
          end
          -- 2 枚目以降の窓は、分割の前にセル数で保存時の大きさにしておく。
          -- 最大化は GUI の窓ができてから (window-config-reloaded / update-status で) 行う
          local size = win_spec.size
          if size then
            o.width = size.cols
            o.height = size.rows
          end
          local t, p, w = wezterm.mux.spawn_window(o)
          if size and size.maximized then maximize_on_open[w:window_id()] = true end
          target = w
          return t, p
        end)
        for _, item in ipairs(placed) do
          table.insert(all_placed, item)
          if item.spec.active then table.insert(actives, item.pane) end
        end
        if tab_spec.active then active_tab = tab end
      end
    end
    if active_tab then table.insert(actives, active_tab) end
  end

  if fresh then
    -- CloseCurrentPane は渡したペインではなくアクティブなペインを閉じるため、先に初期ペインをアクティブにする
    pane:tab():activate()
    pane:activate()
    window:perform_action(wezterm.action.CloseCurrentPane({ confirm = false }), pane)
  end
  for _, obj in ipairs(actives) do obj:activate() end
  verify_list = {}
  for _, item in ipairs(all_placed) do
    if item.spec.resumable then
      table.insert(verify_list, { pane_id = item.pane:pane_id(), session_id = item.spec.claude.id, cwd = item.spec.cwd })
    end
  end
  verify_at = os.time() + math.ceil(resumed * RESUME_STAGGER_MS / 1000) + 20
  verify_stage = 0
  window:toast_notification(
    "WezTerm",
    string.format("%s の状態を復元: %d ペイン (Claude %d)", os.date("%m/%d %H:%M", snap.saved_at), #all_placed, resumed),
    nil,
    5000
  )
end

-- 窓の大きさを合わせる途中の復元。window_id → 保存。window-resized を受けたら分割を作る
local pending = {}

local function size_matches(window, spec)
  local tab = window:active_tab()
  local size = tab and tab:get_size()
  return size ~= nil and math.abs(size.cols - spec.cols) <= 1 and math.abs(size.rows - spec.rows) <= 1
end

-- 分割は窓の大きさに対する比率で作るが、作った後に窓を広げると WezTerm は比率どおりには
-- 配らない (実測)。そのため先に窓を保存時の大きさにし、window-resized を待ってから分割する
function M.restore(window, pane)
  local snap = pick_previous()
  if not snap then
    window:toast_notification("WezTerm", "復元できる前回の保存がない", nil, 4000)
    return
  end
  local id = window:window_id()
  local spec = snap.windows and snap.windows[1] and snap.windows[1].size
  -- 大きさが変わらずイベントが来なかったときは、もう一度押すとそのまま作る
  if spec and not pending[id] and not size_matches(window, spec) then
    pending[id] = snap
    if spec.maximized then
      window:maximize()
    else
      window:set_inner_size(spec.pixel_width, spec.pixel_height)
    end
    return
  end
  pending[id] = nil
  build(window, pane, snap)
end

-- user_opts.pane_args(cwd, session_id, delay_ms) -> 復元する各ペインの起動コマンド (argv) または nil
function M.setup(user_opts)
  for k, v in pairs(user_opts or {}) do opts[k] = v end
  wezterm.on("window-resized", function(window, pane)
    local snap = pending[window:window_id()]
    if not snap then return end
    pending[window:window_id()] = nil
    local ok, err = pcall(build, window, pane, snap)
    if not ok then
      wezterm.log_error("session_restore: restore failed: " .. tostring(err))
      window:toast_notification("WezTerm", "復元に失敗: " .. tostring(err), nil, 8000)
    end
  end)
  local function maximize_if_pending(window)
    local id = window:window_id()
    if maximize_on_open[id] then
      maximize_on_open[id] = nil
      window:maximize()
    end
  end
  wezterm.on("update-status", function(window)
    maximize_if_pending(window)
    local ok, err = pcall(verify_resumed, window)
    if not ok then wezterm.log_error("session_restore: verify failed: " .. tostring(err)) end
  end)
  wezterm.on("window-config-reloaded", function(window)
    maximize_if_pending(window)
    if loop_started then return end
    local ok, err = pcall(function()
      -- 保存するのは GUI プロセスだけ。このイベントは GUI の窓でしか発火しないが、wezterm.gui を
      -- 持たない評価 (wezterm-mux-server 等) で保存を始めないよう、念のため確かめる
      if wezterm.gui then
        loop_started = true
        start_saving()
      end
    end)
    if not ok then wezterm.log_error("session_restore: setup failed: " .. tostring(err)) end
  end)
end

function M.restore_action()
  return wezterm.action_callback(function(window, pane)
    local ok, err = pcall(M.restore, window, pane)
    if not ok then
      wezterm.log_error("session_restore: restore failed: " .. tostring(err))
      window:toast_notification("WezTerm", "復元に失敗: " .. tostring(err), nil, 8000)
    end
  end)
end

return M
