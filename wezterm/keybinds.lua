local wezterm = require("wezterm")
local act = wezterm.action
local session_restore = require("session_restore")

-- wezterm cli の実行ファイル。GUI と同じフォルダの本体を直接呼び、scoop の shim を経由する
-- 分のプロセス起動を省く。見つからなければ PATH 上の wezterm に任せる
local wezterm_exe = (function()
  local is_windows = wezterm.target_triple:find("windows") ~= nil
  local sep = is_windows and "\\" or "/"
  local path = wezterm.executable_dir .. sep .. (is_windows and "wezterm.exe" or "wezterm")
  local f = io.open(path, "rb")
  if f then
    f:close()
    return path
  end
  return "wezterm"
end)()

-- `wezterm cli <args...>` を実行し、成否を返す。失敗時は stderr をログに出す
local function cli(args)
  local argv = { wezterm_exe, "cli" }
  for _, a in ipairs(args) do argv[#argv + 1] = a end
  local ok, _, stderr = wezterm.run_child_process(argv)
  if not ok then
    wezterm.log_error("wezterm cli " .. table.concat(args, " ") .. " failed: " .. tostring(stderr))
  end
  return ok
end

-- ペインとそのタブにフォーカスを移す
local function focus_pane(pane_id)
  local p = wezterm.mux.get_pane(pane_id)
  if not p then return end
  local t = p:tab()
  if t then t:activate() end
  p:activate()
end

-- tab:panes() の並びで pane の位置を返す (見つからなければ nil)
local function pane_index(panes, pane_id)
  for i, p in ipairs(panes) do
    if p:pane_id() == pane_id then return i end
  end
  return nil
end

-- ペイン/タブ統合ナビゲーション。delta=1 で次へ、-1 で前へ。端のペインではタブを移る
local function navigate(delta)
  return wezterm.action_callback(function(window, pane)
    local panes = window:active_tab():panes()

    if #panes == 1 then
      window:perform_action(act.ActivateTabRelative(delta), pane)
      return
    end

    local idx = pane_index(panes, pane:pane_id())
    if idx == nil then return end

    if (delta > 0 and idx == #panes) or (delta < 0 and idx == 1) then
      window:perform_action(act.ActivateTabRelative(delta), pane)
    else
      window:perform_action(act.ActivatePaneDirection(delta > 0 and "Next" or "Prev"), pane)
    end
  end)
end

-- 現在のタブの全ペインを左隣のタブに統合する。T (左隣のアクティブペイン) の右に、
-- tab:panes() の順で A|B|C とつなげ、全ペインの幅をそろえる
local merge_adjacent_tab = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local tabs = tab:window():tabs()

  local current_tab_idx = nil
  for i, t in ipairs(tabs) do
    if t:tab_id() == tab:tab_id() then
      current_tab_idx = i
      break
    end
  end

  -- 左隣のタブが存在しない場合は何もしない
  if current_tab_idx == nil or current_tab_idx <= 1 then
    return
  end

  local target_pane_id = tabs[current_tab_idx - 1]:active_pane():pane_id()
  local active_pane_id = pane:pane_id()

  -- 最後のペインを移すとタブが消えるので、先に ID だけを控える
  local ids = {}
  for _, p in ipairs(tab:panes()) do ids[#ids + 1] = p:pane_id() end

  -- i 番目の split で移したペインに直前のセルの (n-i+1)/(n-i+2) を渡すと、
  -- T を含む n+1 個のペインがほぼ等幅になる (n=3 なら 75, 67, 50)
  local n = #ids
  for i, id in ipairs(ids) do
    local pct = math.floor(100 * (n - i + 1) / (n - i + 2) + 0.5)
    if not cli({
      "split-pane",
      "--move-pane-id", tostring(id),
      "--pane-id", tostring(target_pane_id),
      "--right", "--percent", tostring(pct),
    }) then
      return
    end
    target_pane_id = id
  end

  focus_pane(active_pane_id)
end)

-- 隣のペインを画面上の配置から探す。dir は "left" / "right" / "up" / "down"。
-- 辺を丸ごと共有するペイン (左右なら同じ top と height、上下なら同じ left と width) だけを隣とみなす。
-- 部分的に接するペインと入れ替えると分割の木が作り替わるため対象外
local function find_neighbor(infos, cur, dir)
  local function touches(a_start, a_len, b_start)
    -- 区切り線の幅は 0 か 1 セル
    local gap = b_start - (a_start + a_len)
    return gap == 0 or gap == 1
  end
  for _, o in ipairs(infos) do
    if o.pane:pane_id() ~= cur.pane:pane_id() then
      local same_row = o.top == cur.top and o.height == cur.height
      local same_col = o.left == cur.left and o.width == cur.width
      if (dir == "left" and same_row and touches(o.left, o.width, cur.left))
        or (dir == "right" and same_row and touches(cur.left, cur.width, o.left))
        or (dir == "up" and same_col and touches(o.top, o.height, cur.top))
        or (dir == "down" and same_col and touches(cur.top, cur.height, o.top)) then
        return o
      end
    end
  end
  return nil
end

-- 隣のペインと位置を入れ替える。delta=-1 は左 (なければ上)、1 は右 (なければ下)。
-- 隣がなければタブ自体を delta 方向へ動かす。サイズは 2 ペインの合計に対する今の比で保つ
local function swap(delta)
  return wezterm.action_callback(function(window, pane)
    local tab = window:active_tab()
    local infos = tab:panes_with_info()
    local current_id = pane:pane_id()

    local cur = nil
    for _, pi in ipairs(infos) do
      if pi.pane:pane_id() == current_id then cur = pi; break end
    end
    if cur == nil then return end

    local candidates = delta < 0
      and { { "left", "--left", "width" }, { "up", "--top", "height" } }
      or { { "right", "--right", "width" }, { "down", "--bottom", "height" } }

    for _, c in ipairs(candidates) do
      local nbr = find_neighbor(infos, cur, c[1])
      if nbr then
        local size = c[3]
        local pct = math.floor(cur[size] / (cur[size] + nbr[size]) * 100 + 0.5)
        if cli({
          "split-pane",
          "--move-pane-id", tostring(current_id),
          "--pane-id", tostring(nbr.pane:pane_id()),
          c[2], "--percent", tostring(pct),
        }) then
          focus_pane(current_id)
        end
        return
      end
    end

    window:perform_action(act.MoveTabRelative(delta), pane)
  end)
end

-- 現在のペインを新規タブに分離
local split_pane_to_tab = wezterm.action_callback(function(window, pane)
  pane:move_to_new_tab()
end)

return {
  keys = {
    -- タブ/ペイン統合ナビゲーション
    { key = "Tab", mods = "CTRL", action = navigate(1) },
    { key = "Tab", mods = "SHIFT|CTRL", action = navigate(-1) },
    -- タブ新規作成 (default_prog で開き、cwd は現在ペインから引き継ぐ)
    { key = "t", mods = "CTRL", action = act.SpawnTab("CurrentPaneDomain") },
    -- 新規ウィンドウ
    { key = "n", mods = "SHIFT|CTRL", action = act.SpawnWindow },
    -- nvimを新規ウィンドウで開く（現在ペインのCWDを引き継ぐ）
    {
      key = "e",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        window:perform_action(act.SpawnCommandInNewWindow({
          args = { "nvim" },
          cwd = session_restore.pane_cwd(pane),
        }), pane)
      end),
    },
    -- タブを閉じる
    { key = "w", mods = "CTRL", action = act({ CloseCurrentTab = { confirm = true } }) },
    -- タブ/ペイン位置入れ替え（隣のペインとスワップ、隣がなければタブ移動）
    { key = ",", mods = "ALT", action = swap(-1) },
    { key = ".", mods = "ALT", action = swap(1) },

    -- タブ合成・分離
    { key = "m", mods = "ALT", action = merge_adjacent_tab },
    { key = "M", mods = "ALT|SHIFT", action = split_pane_to_tab },

    -- Enter: Win32 Input Mode のエスケープシーケンス漏れ対策
    { key = "Enter", mods = "NONE", action = act.SendString("\r") },
    -- コピーモード
    { key = "v", mods = "ALT", action = act.ActivateCopyMode },
    -- コピー（選択範囲がある場合）またはターミナル停止（選択範囲がない場合）
    {
      key = "c",
      mods = "CTRL",
      action = wezterm.action_callback(function(window, pane)
        local has_selection = window:get_selection_text_for_pane(pane) ~= ""
        if has_selection then
          window:perform_action(act.CopyTo("Clipboard"), pane)
        else
          window:perform_action(act.SendKey({ key = "c", mods = "CTRL" }), pane)
        end
      end),
    },
    -- 貼り付け
    { key = "v", mods = "CTRL", action = act.PasteFrom("Clipboard") },

    -- ペイン作成 (default_prog で開き、cwd は現在ペインから引き継ぐ)
    { key = "d", mods = "ALT", action = act.SplitVertical({ domain = "CurrentPaneDomain" }) },
    { key = "r", mods = "ALT", action = act.SplitHorizontal({ domain = "CurrentPaneDomain" }) },
    -- ペインを閉じる
    { key = "x", mods = "ALT", action = act({ CloseCurrentPane = { confirm = true } }) },
    -- ペイン移動 (Alt+矢印。Alt+hjkl は nvim の編集操作用に解放)
    { key = "LeftArrow", mods = "ALT", action = act.ActivatePaneDirection("Left") },
    { key = "RightArrow", mods = "ALT", action = act.ActivatePaneDirection("Right") },
    { key = "UpArrow", mods = "ALT", action = act.ActivatePaneDirection("Up") },
    { key = "DownArrow", mods = "ALT", action = act.ActivatePaneDirection("Down") },
    -- ペインズーム
    { key = "z", mods = "ALT", action = act.TogglePaneZoomState },
    -- 前回の WezTerm の窓・タブ・ペインと Claude Code の会話を復元
    { key = "R", mods = "ALT|SHIFT", action = session_restore.restore_action() },
    -- フォントサイズ
    { key = "+", mods = "CTRL", action = act.IncreaseFontSize },
    { key = "-", mods = "CTRL", action = act.DecreaseFontSize },
    { key = "0", mods = "CTRL", action = act.ResetFontSize },

    -- デバッグオーバーレイ
    { key = "L", mods = "SHIFT|CTRL", action = act.ShowDebugOverlay },
    -- タブ切替 Ctrl + 数字
    { key = "1", mods = "CTRL", action = act.ActivateTab(0) },
    { key = "2", mods = "CTRL", action = act.ActivateTab(1) },
    { key = "3", mods = "CTRL", action = act.ActivateTab(2) },
    { key = "4", mods = "CTRL", action = act.ActivateTab(3) },
    { key = "5", mods = "CTRL", action = act.ActivateTab(4) },
    { key = "6", mods = "CTRL", action = act.ActivateTab(5) },
    { key = "7", mods = "CTRL", action = act.ActivateTab(6) },
    { key = "8", mods = "CTRL", action = act.ActivateTab(7) },
    { key = "9", mods = "CTRL", action = act.ActivateTab(-1) },

    -- コマンドパレット
    { key = "p", mods = "SHIFT|CTRL", action = act.ActivateCommandPalette },
    -- 設定再読み込み
    { key = "r", mods = "SHIFT|CTRL", action = act.ReloadConfiguration },
    -- ペインサイズ調整モード
    { key = "s", mods = "ALT", action = act.ActivateKeyTable({ name = "resize_pane", one_shot = false }) },
  },
  -- キーテーブル
  key_tables = {
    -- ペインサイズ調整 Alt+s
    resize_pane = {
      { key = "h", action = act.AdjustPaneSize({ "Left", 1 }) },
      { key = "l", action = act.AdjustPaneSize({ "Right", 1 }) },
      { key = "k", action = act.AdjustPaneSize({ "Up", 1 }) },
      { key = "j", action = act.AdjustPaneSize({ "Down", 1 }) },
      { key = "Enter", action = "PopKeyTable" },
      { key = "Escape", action = "PopKeyTable" },
    },
    -- コピーモード Alt+v
    copy_mode = {
      -- 移動
      { key = "h", mods = "NONE", action = act.CopyMode("MoveLeft") },
      { key = "j", mods = "NONE", action = act.CopyMode("MoveDown") },
      { key = "k", mods = "NONE", action = act.CopyMode("MoveUp") },
      { key = "l", mods = "NONE", action = act.CopyMode("MoveRight") },
      -- 最初と最後に移動
      { key = "^", mods = "NONE", action = act.CopyMode("MoveToStartOfLineContent") },
      { key = "$", mods = "NONE", action = act.CopyMode("MoveToEndOfLineContent") },
      -- 左端に移動
      { key = "0", mods = "NONE", action = act.CopyMode("MoveToStartOfLine") },
      { key = "o", mods = "NONE", action = act.CopyMode("MoveToSelectionOtherEnd") },
      { key = "O", mods = "NONE", action = act.CopyMode("MoveToSelectionOtherEndHoriz") },
      --
      { key = ";", mods = "NONE", action = act.CopyMode("JumpAgain") },
      -- 単語ごと移動
      { key = "w", mods = "NONE", action = act.CopyMode("MoveForwardWord") },
      { key = "b", mods = "NONE", action = act.CopyMode("MoveBackwardWord") },
      { key = "e", mods = "NONE", action = act.CopyMode("MoveForwardWordEnd") },
      -- ジャンプ機能 t f
      { key = "t", mods = "NONE", action = act.CopyMode({ JumpForward = { prev_char = true } }) },
      { key = "f", mods = "NONE", action = act.CopyMode({ JumpForward = { prev_char = false } }) },
      { key = "T", mods = "NONE", action = act.CopyMode({ JumpBackward = { prev_char = true } }) },
      { key = "F", mods = "NONE", action = act.CopyMode({ JumpBackward = { prev_char = false } }) },
      -- 一番下へ
      { key = "G", mods = "NONE", action = act.CopyMode("MoveToScrollbackBottom") },
      -- 一番上へ
      { key = "g", mods = "NONE", action = act.CopyMode("MoveToScrollbackTop") },
      -- viewport
      { key = "H", mods = "NONE", action = act.CopyMode("MoveToViewportTop") },
      { key = "L", mods = "NONE", action = act.CopyMode("MoveToViewportBottom") },
      { key = "M", mods = "NONE", action = act.CopyMode("MoveToViewportMiddle") },
      -- スクロール
      { key = "b", mods = "CTRL", action = act.CopyMode("PageUp") },
      { key = "f", mods = "CTRL", action = act.CopyMode("PageDown") },
      { key = "d", mods = "CTRL", action = act.CopyMode({ MoveByPage = 0.5 }) },
      { key = "u", mods = "CTRL", action = act.CopyMode({ MoveByPage = -0.5 }) },
      -- 範囲選択モード
      { key = "v", mods = "NONE", action = act.CopyMode({ SetSelectionMode = "Cell" }) },
      { key = "v", mods = "CTRL", action = act.CopyMode({ SetSelectionMode = "Block" }) },
      { key = "V", mods = "NONE", action = act.CopyMode({ SetSelectionMode = "Line" }) },
      -- コピー
      { key = "y", mods = "NONE", action = act.CopyTo("Clipboard") },
      -- コピーモードを終了
      {
        key = "Enter",
        mods = "NONE",
        action = act.Multiple({ { CopyTo = "ClipboardAndPrimarySelection" }, { CopyMode = "Close" } }),
      },
      { key = "Escape", mods = "NONE", action = act.CopyMode("Close") },
      { key = "c", mods = "CTRL", action = act.CopyMode("Close") },
      { key = "q", mods = "NONE", action = act.CopyMode("Close") },
    },
  },
}
