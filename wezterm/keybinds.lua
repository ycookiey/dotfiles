local wezterm = require("wezterm")
local act = wezterm.action

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

-- ペイン幅を均等化
local equalize_panes = wezterm.action_callback(function(window, pane)
  local tab = window:active_tab()
  local panes_info = tab:panes_with_info()
  if #panes_info <= 1 then return end

  table.sort(panes_info, function(a, b) return a.left < b.left end)

  local total_width = 0
  for _, info in ipairs(panes_info) do
    total_width = total_width + info.width
  end

  local target = math.floor(total_width / #panes_info)
  local cumulative_shift = 0

  for i = 1, #panes_info - 1 do
    local effective_width = panes_info[i].width - cumulative_shift
    local diff = target - effective_width
    if diff > 0 then
      window:perform_action(act.AdjustPaneSize({ "Right", diff }), panes_info[i].pane)
    elseif diff < 0 then
      window:perform_action(act.AdjustPaneSize({ "Left", -diff }), panes_info[i + 1].pane)
    end
    cumulative_shift = cumulative_shift + diff
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
    -- タブ新規作成
    { key = "t", mods = "CTRL", action = act({ SpawnTab = "CurrentPaneDomain" }) },
    -- タブを閉じる
    { key = "w", mods = "CTRL", action = act({ CloseCurrentTab = { confirm = true } }) },
    -- タブ位置入れ替え
    { key = ",", mods = "ALT", action = act({ MoveTabRelative = -1 }) },
    { key = ".", mods = "ALT", action = act({ MoveTabRelative = 1 }) },

    -- タブ合成・分離
    { key = "m", mods = "ALT", action = merge_adjacent_tab },
    { key = "M", mods = "ALT|SHIFT", action = split_pane_to_tab },

    -- フルスクリーン
    { key = "Enter", mods = "ALT", action = act.ToggleFullScreen },

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

    -- ペイン作成
    { key = "d", mods = "ALT", action = act.SplitVertical({ domain = "CurrentPaneDomain" }) },
    { key = "r", mods = "ALT", action = act.SplitHorizontal({ domain = "CurrentPaneDomain" }) },
    -- ペインを閉じる
    { key = "x", mods = "ALT", action = act({ CloseCurrentPane = { confirm = true } }) },
    -- ペイン移動
    { key = "h", mods = "ALT", action = act.ActivatePaneDirection("Left") },
    { key = "l", mods = "ALT", action = act.ActivatePaneDirection("Right") },
    { key = "k", mods = "ALT", action = act.ActivatePaneDirection("Up") },
    { key = "j", mods = "ALT", action = act.ActivatePaneDirection("Down") },
    -- ペインズーム
    { key = "z", mods = "ALT", action = act.TogglePaneZoomState },
    -- ペイン幅均等化
    { key = "S", mods = "ALT|SHIFT", action = equalize_panes },

    -- フォントサイズ
    { key = "+", mods = "CTRL", action = act.IncreaseFontSize },
    { key = "-", mods = "CTRL", action = act.DecreaseFontSize },
    { key = "0", mods = "CTRL", action = act.ResetFontSize },

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
