local wezterm = require("wezterm")
local config = wezterm.config_builder()

----------------------------------------------------
-- 基本設定
----------------------------------------------------
config.automatically_reload_config = true
config.use_ime = true

-- デフォルトをPowerShellに設定
config.default_prog = { 'pwsh' }

----------------------------------------------------
-- フォント設定
config.font = wezterm.font('HackGen Console NF')
config.font_size = 12.0

----------------------------------------------------
-- カラー設定
----------------------------------------------------
config.color_scheme = 'Tokyo Night'

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
  colors = { "#000000" },
}

-- タブの追加ボタンを非表示
config.show_new_tab_button_in_tab_bar = false
-- nightlyのみ使用可能
-- タブの閉じるボタンを非表示
config.show_close_tab_button_in_tabs = false

config.colors = {
  foreground = "#ffffff",
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
  local background
  local foreground
  local edge_background = "none"
  local title

  if tab.is_active then
    -- アクティブタブ
    background = "#6c7086"
    foreground = "#ffffff"

    -- アクティブタブの幅を30文字に固定（中央揃え）
    local fixed_width = 30
    title = wezterm.truncate_right(tab.active_pane.title, fixed_width)

    local title_width = wezterm.column_width(title)
    local padding_total = fixed_width - title_width
    local padding_left = math.floor(padding_total / 2)
    local padding_right = padding_total - padding_left

    title = string.rep(" ", padding_left) .. title .. string.rep(" ", padding_right)
  else
    -- 非アクティブタブ
    background = "#45475a"
    foreground = "#959cb4"

    -- 非アクティブタブは通常の幅
    title = wezterm.truncate_right(tab.active_pane.title, max_width - 1)
  end

  local edge_foreground = background
  return {
    { Background = { Color = edge_background } },
    { Foreground = { Color = edge_foreground } },
    { Text = SOLID_LEFT_ARROW },
    { Background = { Color = background } },
    { Foreground = { Color = foreground } },
    { Text = title },
    { Background = { Color = edge_background } },
    { Foreground = { Color = edge_foreground } },
    { Text = SOLID_RIGHT_ARROW },
  }
end)

----------------------------------------------------
-- keybinds
----------------------------------------------------
config.disable_default_key_bindings = true
config.keys = require("keybinds").keys
config.key_tables = require("keybinds").key_tables
config.leader = { key = "q", mods = "CTRL", timeout_milliseconds = 2000 }

return config
