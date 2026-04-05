local wezterm = require("wezterm")
local act = wezterm.action

-- フォーカス中のペインのプロセス名を取得
local function fg_name(pane)
  local proc = pane:get_foreground_process_name()
  if not proc then return "" end
  return proc:match("([^/\\]+)$") or ""
end

local function is_nushell(pane)
  local name = fg_name(pane)
  return name == "nu.exe" or name == "nu"
end

local function is_nvim(pane)
  local name = fg_name(pane)
  if name == "nvim.exe" or name == "nvim" then return true end
  -- フォールバック: タイトルで判定
  local title = (pane:get_title() or ""):lower()
  return title:find("nvim") ~= nil
end

local function is_claude_code(pane)
  local name = fg_name(pane)
  return name == "claude.exe" or name == "claude"
end

-- nvimからのペイン移動要求を処理（user-var経由）
wezterm.on("user-var-changed", function(window, pane, name, value)
  if name == "pane_right" then
    window:perform_action(act.ActivatePaneDirection("Right"), pane)
  end
end)

-- Show which key table is active in the status area
wezterm.on("update-right-status", function(window, pane)
  local name = window:active_key_table()
  if name then
    name = "TABLE: " .. name
  end
  window:set_right_status(name or "")
end)

-- ペイン/タブ統合ナビゲーション（次へ）
local navigate_next = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local panes = tab:panes()

  if #panes == 1 then
    window:perform_action(act.ActivateTabRelative(1), pane)
    return
  end

  local current_id = pane:pane_id()
  local current_idx = nil
  for i, p in ipairs(panes) do
    if p:pane_id() == current_id then
      current_idx = i
      break
    end
  end

  if current_idx == #panes then
    window:perform_action(act.ActivateTabRelative(1), pane)
  else
    window:perform_action(act.ActivatePaneDirection("Next"), pane)
  end
end)

-- ペイン/タブ統合ナビゲーション（前へ）
local navigate_prev = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local panes = tab:panes()

  if #panes == 1 then
    window:perform_action(act.ActivateTabRelative(-1), pane)
    return
  end

  local current_id = pane:pane_id()
  local current_idx = nil
  for i, p in ipairs(panes) do
    if p:pane_id() == current_id then
      current_idx = i
      break
    end
  end

  if current_idx == 1 then
    window:perform_action(act.ActivateTabRelative(-1), pane)
  else
    window:perform_action(act.ActivatePaneDirection("Prev"), pane)
  end
end)

-- 現在のタブの全ペインを左隣のタブに統合
local merge_adjacent_tab = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local mux_window = tab:window()
  local tabs = mux_window:tabs()

  -- 現在のタブインデックスを取得
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

  -- 左隣のタブのアクティブペインIDを取得
  local prev_tab = tabs[current_tab_idx - 1]
  local target_pane_id = prev_tab:active_pane():pane_id()

  -- 直前のアクティブペインIDを記録
  local active_pane_id = pane:pane_id()

  -- 現在のタブの全ペインを左隣に統合
  local current_panes = tab:panes()
  for _, p in ipairs(current_panes) do
    wezterm.run_child_process({
      "wezterm", "cli", "split-pane",
      "--move-pane-id", tostring(p:pane_id()),
      "--right",
      "--pane-id", tostring(target_pane_id),
    })
  end

  -- 元のアクティブペインにフォーカスを戻す
  wezterm.run_child_process({
    "wezterm", "cli", "activate-pane",
    "--pane-id", tostring(active_pane_id),
  })
end)

-- ペイン幅を均等化（毎回再取得して1境界ずつ調整）
local equalize_panes = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local panes_info = tab:panes_with_info()
  if #panes_info <= 1 then return end

  local n = #panes_info
  table.sort(panes_info, function(a, b) return a.left < b.left end)

  local total_width = 0
  for _, info in ipairs(panes_info) do
    total_width = total_width + info.width
  end
  local target = math.floor(total_width / n)

  for i = 1, n - 1 do
    -- 各境界調整前に最新の状態を取得
    local fresh = tab:panes_with_info()
    table.sort(fresh, function(a, b) return a.left < b.left end)

    local diff = fresh[i].width - target
    if diff > 0 then
      -- i番目が大きすぎる → i+1番目を左に広げて縮める
      window:perform_action(act.AdjustPaneSize({ "Left", diff }), fresh[i + 1].pane)
    elseif diff < 0 then
      -- i番目が小さすぎる → i番目を右に広げる
      window:perform_action(act.AdjustPaneSize({ "Right", -diff }), fresh[i].pane)
    end
  end
end)

-- 現在のペインを新規タブに分離
local split_pane_to_tab = wezterm.action_callback(function(window, pane)
  pane:move_to_new_tab()
end)

return {
  keys = {
    -- タブ/ペイン統合ナビゲーション
    { key = "Tab", mods = "CTRL", action = navigate_next },
    { key = "Tab", mods = "SHIFT|CTRL", action = navigate_prev },
    -- タブ新規作成（nushellペインならnushellで開く）
    {
      key = "t",
      mods = "CTRL",
      action = wezterm.action_callback(function(window, pane)
        if is_nushell(pane) then
          window:perform_action(act.SpawnCommandInNewTab({ args = { "nu" } }), pane)
        else
          window:perform_action(act.SpawnTab("CurrentPaneDomain"), pane)
        end
      end),
    },
    -- nushellタブ
    { key = "n", mods = "SHIFT|CTRL", action = act.SpawnCommandInNewTab({ args = { "nu" } }) },
    -- タブを閉じる
    { key = "w", mods = "CTRL", action = act({ CloseCurrentTab = { confirm = true } }) },
    -- タブ/ペイン位置入れ替え（隣のペインとスワップ、端なら外へ → タブ移動）
    {
      key = ",",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        local tab = window:active_tab()
        local panes = tab:panes()
        local current_id = pane:pane_id()
        local idx = nil
        for i, p in ipairs(panes) do
          if p:pane_id() == current_id then idx = i; break end
        end
        if #panes <= 1 or idx == 1 then
          window:perform_action(act.MoveTabRelative(-1), pane)
        else
          local info = tab:panes_with_info()
          local cur_w, nbr_w
          for _, pi in ipairs(info) do
            if pi.pane:pane_id() == current_id then cur_w = pi.width end
            if pi.pane:pane_id() == panes[idx - 1]:pane_id() then nbr_w = pi.width end
          end
          local pct = math.floor(cur_w / (cur_w + nbr_w) * 100 + 0.5)
          wezterm.run_child_process({
            "wezterm", "cli", "split-pane",
            "--move-pane-id", tostring(current_id),
            "--pane-id", tostring(panes[idx - 1]:pane_id()),
            "--left", "--percent", tostring(pct),
          })
          wezterm.run_child_process({
            "wezterm", "cli", "activate-pane",
            "--pane-id", tostring(current_id),
          })
        end
      end),
    },
    {
      key = ".",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        local tab = window:active_tab()
        local panes = tab:panes()
        local current_id = pane:pane_id()
        local idx = nil
        for i, p in ipairs(panes) do
          if p:pane_id() == current_id then idx = i; break end
        end
        if #panes <= 1 or idx == #panes then
          window:perform_action(act.MoveTabRelative(1), pane)
        else
          local info = tab:panes_with_info()
          local cur_w, nbr_w
          for _, pi in ipairs(info) do
            if pi.pane:pane_id() == current_id then cur_w = pi.width end
            if pi.pane:pane_id() == panes[idx + 1]:pane_id() then nbr_w = pi.width end
          end
          local pct = math.floor(cur_w / (cur_w + nbr_w) * 100 + 0.5)
          wezterm.run_child_process({
            "wezterm", "cli", "split-pane",
            "--move-pane-id", tostring(current_id),
            "--pane-id", tostring(panes[idx + 1]:pane_id()),
            "--right", "--percent", tostring(pct),
          })
          wezterm.run_child_process({
            "wezterm", "cli", "activate-pane",
            "--pane-id", tostring(current_id),
          })
        end
      end),
    },

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

    -- ペイン作成（nushellペインならnushellで開く）
    {
      key = "d",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        local split = is_nushell(pane)
          and act.SplitVertical({ args = { "nu" }, domain = "CurrentPaneDomain" })
          or act.SplitVertical({ domain = "CurrentPaneDomain" })
        window:perform_action(split, pane)
      end),
    },
    {
      key = "r",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        local split = is_nushell(pane)
          and act.SplitHorizontal({ args = { "nu" }, domain = "CurrentPaneDomain" })
          or act.SplitHorizontal({ domain = "CurrentPaneDomain" })
        window:perform_action(split, pane)
      end),
    },
    -- ペインを閉じる
    { key = "x", mods = "ALT", action = act({ CloseCurrentPane = { confirm = true } }) },
    -- ペイン移動
    { key = "h", mods = "ALT", action = act.ActivatePaneDirection("Left") },
    {
      key = "l",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        if is_nvim(pane) then
          window:perform_action(act.SendKey({ key = "l", mods = "ALT" }), pane)
        else
          window:perform_action(act.ActivatePaneDirection("Right"), pane)
        end
      end),
    },
    { key = "k", mods = "ALT", action = act.ActivatePaneDirection("Up") },
    { key = "j", mods = "ALT", action = act.ActivatePaneDirection("Down") },
    -- ペインズーム
    {
      key = "z",
      mods = "ALT",
      action = wezterm.action_callback(function(window, pane)
        window:perform_action(act.TogglePaneZoomState, pane)
        if is_claude_code(pane) then
          window:perform_action(act.SendKey({ key = "l", mods = "CTRL" }), pane)
        end
      end),
    },
    -- ペイン幅均等化
    { key = "S", mods = "ALT|SHIFT", action = equalize_panes },

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
