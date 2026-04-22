local wezterm = require("wezterm")
local config = wezterm.config_builder()

local function detect_is_dark()
  return wezterm.gui.get_appearance():find("Dark") ~= nil
end

local is_dark = detect_is_dark()

-- nvim連携: テーマをファイルに書き出す（config reload = OSテーマ変更時に更新）
local home = os.getenv("USERPROFILE") or os.getenv("HOME") or ""
local f = io.open(home .. "\\.wezterm_appearance", "w")
if f then
  f:write(is_dark and "dark" or "light")
  f:close()
end

-- 新規ウィンドウ/リロード時に最新のOSテーマを各ウィンドウへ適用
-- （is_dark はプロセス起動時の値で固定されるため、window_config_reloaded でoverride）
wezterm.on("window-config-reloaded", function(window)
  local dark = detect_is_dark()
  local overrides = window:get_config_overrides() or {}
  local scheme = dark and "Tokyo Night" or "Tokyo Night Day"
  local grad = dark and "#000000" or "#e1e2e7"
  local fg = dark and "#ffffff" or "#3760bf"

  local changed = false
  if overrides.color_scheme ~= scheme then
    overrides.color_scheme = scheme
    changed = true
  end
  local cur_grad = overrides.window_background_gradient
    and overrides.window_background_gradient.colors
    and overrides.window_background_gradient.colors[1]
  if cur_grad ~= grad then
    overrides.window_background_gradient = { colors = { grad } }
    changed = true
  end
  local cur_fg = overrides.colors and overrides.colors.foreground
  if cur_fg ~= fg then
    overrides.colors = { foreground = fg, tab_bar = { inactive_tab_edge = "none" } }
    changed = true
  end

  if changed then
    window:set_config_overrides(overrides)
    -- appearanceファイルも同期
    local fp = io.open(home .. "\\.wezterm_appearance", "w")
    if fp then fp:write(dark and "dark" or "light"); fp:close() end
  end
end)

----------------------------------------------------
-- 基本設定
----------------------------------------------------
config.automatically_reload_config = true
config.use_ime = true

-- デフォルトをNushellに設定
config.default_prog = { 'nu' }
config.default_cwd = [[C:\Main\Project]]

----------------------------------------------------
-- フォント設定
config.font = wezterm.font('HackGen Console NF')
config.font_size = 12.0

----------------------------------------------------
-- カラー設定
----------------------------------------------------
config.color_scheme = is_dark and 'Tokyo Night' or 'Tokyo Night Day'

----------------------------------------------------
-- カーソル設定
----------------------------------------------------
config.default_cursor_style = 'BlinkingBlock'
config.cursor_blink_rate = 300

----------------------------------------------------
-- ウィンドウ・背景設定
----------------------------------------------------
config.window_background_opacity = 0.8

----------------------------------------------------
-- パフォーマンス設定
----------------------------------------------------
config.front_end = 'OpenGL'
config.webgpu_power_preference = 'HighPerformance'
config.scrollback_lines = 20000

----------------------------------------------------
-- ペイン設定
----------------------------------------------------
config.inactive_pane_hsb = {
  saturation = 0.7,
  brightness = 0.4,
}

----------------------------------------------------
-- Tab
----------------------------------------------------
-- タイトルバーを非表示
config.window_decorations = "RESIZE"
-- タブバーの表示
config.show_tabs_in_tab_bar = true
-- タブが一つの時は非表示
config.hide_tab_bar_if_only_one_tab = true
-- falseにするとタブバーの透過が効かなくなる
-- config.use_fancy_tab_bar = false

-- タブバーの透過
config.window_frame = {
  inactive_titlebar_bg = "none",
  active_titlebar_bg = "none",
}

-- タブバーを背景色に合わせる
config.window_background_gradient = {
  colors = { is_dark and "#000000" or "#e1e2e7" },
}

-- タブの追加ボタンを非表示
config.show_new_tab_button_in_tab_bar = false
-- nightlyのみ使用可能
-- タブの閉じるボタンを非表示
config.show_close_tab_button_in_tabs = false

config.colors = {
  foreground = is_dark and "#ffffff" or "#3760bf",
-- タブ同士の境界線を非表示
  tab_bar = {
    inactive_tab_edge = "none",
  },
}

-- タブの形をカスタマイズ
-- タブの左側の装飾
local SOLID_LEFT_ARROW = wezterm.nerdfonts.ple_left_half_circle_thick
-- タブの右側の装飾
local SOLID_RIGHT_ARROW = wezterm.nerdfonts.ple_right_half_circle_thick

wezterm.on("format-tab-title", function(tab, tabs, panes, config, hover, max_width)
  local dark = detect_is_dark()
  local edge_background = "none"
  local tab_bg = tab.is_active
    and (dark and "#6c7086" or "#b6bfe2")
    or  (dark and "#45475a" or "#c4c8da")
  local active_fg = dark and "#ffffff" or "#3760bf"
  local inactive_fg = dark and "#959cb4" or "#6172b0"
  local dim_fg = dark and "#6c7086" or "#a1a6c5"

  -- タブの丸括弧（左）
  local elements = {
    { Background = { Color = edge_background } },
    { Foreground = { Color = tab_bg } },
    { Text = SOLID_LEFT_ARROW },
  }

  -- 各ペインのタイトルを表示
  local pane_count = #tab.panes
  for i, p in ipairs(tab.panes) do
    local pane_title = p.title
    -- ペインごとの幅を制限
    local pane_max = math.floor((max_width - pane_count + 1) / pane_count)
    if pane_max < 10 then pane_max = 10 end
    pane_title = wezterm.truncate_right(pane_title, pane_max)

    local fg
    if p.pane_id == tab.active_pane.pane_id then
      fg = active_fg
    else
      fg = tab.is_active and inactive_fg or dim_fg
    end

    table.insert(elements, { Background = { Color = tab_bg } })
    table.insert(elements, { Foreground = { Color = fg } })
    table.insert(elements, { Text = " " .. pane_title .. " " })

    -- ペイン間の区切り
    if i < pane_count then
      table.insert(elements, { Foreground = { Color = dim_fg } })
      table.insert(elements, { Text = "|" })
    end
  end

  -- タブの丸括弧（右）
  table.insert(elements, { Background = { Color = edge_background } })
  table.insert(elements, { Foreground = { Color = tab_bg } })
  table.insert(elements, { Text = SOLID_RIGHT_ARROW })

  return elements
end)


----------------------------------------------------
-- keybinds
----------------------------------------------------
config.disable_default_key_bindings = true
config.keys = require("keybinds").keys
config.key_tables = require("keybinds").key_tables

config.wsl_domains = {}

return config