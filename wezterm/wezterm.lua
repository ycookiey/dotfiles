local wezterm = require("wezterm")
local config = wezterm.config_builder()

-- wezterm.gui を持たない評価 (wezterm-mux-server 等) ではダーク扱い
local function detect_is_dark()
  local appearance = wezterm.gui and wezterm.gui.get_appearance() or "Dark"
  return appearance:find("Dark") ~= nil
end

local is_dark = detect_is_dark()

-- テーマごとの配色。読み込み時に 1 回だけ作り、config 本体・override・タブの描画で共有する
local function make_palette(p)
  p.active_fg = p.fg
  return p
end
local PALETTES = {
  dark = make_palette({
    scheme = "Tokyo Night",
    grad = "#000000",
    fg = "#ffffff",
    tab_active_bg = "#6c7086",
    tab_inactive_bg = "#45475a",
    inactive_fg = "#959cb4",
    dim_fg = "#6c7086",
  }),
  light = make_palette({
    scheme = "Tokyo Night Day",
    grad = "#e1e2e7",
    fg = "#3760bf",
    tab_active_bg = "#b6bfe2",
    tab_inactive_bg = "#c4c8da",
    inactive_fg = "#6172b0",
    dim_fg = "#a1a6c5",
  }),
}
local PALETTE_BY_SCHEME = {
  [PALETTES.dark.scheme] = PALETTES.dark,
  [PALETTES.light.scheme] = PALETTES.light,
}

local function palette(dark)
  return dark and PALETTES.dark or PALETTES.light
end

local function gradient_of(p)
  return { colors = { p.grad } }
end

local function colors_of(p)
  return {
    foreground = p.fg,
    -- タブ同士の境界線を非表示
    tab_bar = { inactive_tab_edge = "none" },
  }
end

-- nvim連携: テーマをファイルに書き出す（config reload = OSテーマ変更時に更新）。
-- 外観を読めない評価 (wezterm.gui が nil) では書かない (既定のダーク扱いで上書きしないため)
local home = os.getenv("USERPROFILE") or os.getenv("HOME") or ""
local written_appearance = nil
local function write_appearance(dark)
  if not wezterm.gui or written_appearance == dark then return end
  local fp = io.open(home .. "\\.wezterm_appearance", "w")
  if fp then
    fp:write(dark and "dark" or "light")
    fp:close()
    written_appearance = dark
  end
end

write_appearance(is_dark)

-- 新規ウィンドウ/リロード時に最新のOSテーマを各ウィンドウへ適用
-- （is_dark はプロセス起動時の値で固定されることがあったため、window-config-reloaded でoverride）。
-- override を適用すると設定ファイルが評価し直されるので、実効設定 (override 込み) の scheme が
-- 目標と違うときだけ適用する。override には配色の 3 項目を常にまとめて入れるので、scheme の比較で足りる
wezterm.on("window-config-reloaded", function(window)
  local dark = detect_is_dark()
  local p = palette(dark)
  write_appearance(dark)
  if window:effective_config().color_scheme == p.scheme then return end
  local overrides = window:get_config_overrides() or {}
  overrides.color_scheme = p.scheme
  overrides.window_background_gradient = gradient_of(p)
  overrides.colors = colors_of(p)
  window:set_config_overrides(overrides)
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
config.color_scheme = palette(is_dark).scheme

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
config.window_background_gradient = gradient_of(palette(is_dark))

-- タブの追加ボタンを非表示
config.show_new_tab_button_in_tab_bar = false
-- nightlyのみ使用可能
-- タブの閉じるボタンを非表示
config.show_close_tab_button_in_tabs = false

config.colors = colors_of(palette(is_dark))

-- タブの形をカスタマイズ
-- タブの左側の装飾
local SOLID_LEFT_ARROW = wezterm.nerdfonts.ple_left_half_circle_thick
-- タブの右側の装飾
local SOLID_RIGHT_ARROW = wezterm.nerdfonts.ple_right_half_circle_thick

-- タブバーを計算し直すたびにタブごとに同期で呼ばれるため、外観の問い合わせ (レジストリの読み取り)
-- はせず、窓の実効設定 (override 込み) の color_scheme から配色を引く
wezterm.on("format-tab-title", function(tab, tabs, panes, config, hover, max_width)
  local p = PALETTE_BY_SCHEME[config.color_scheme] or palette(is_dark)
  local edge_background = "none"
  local tab_bg = tab.is_active and p.tab_active_bg or p.tab_inactive_bg
  local active_fg = p.active_fg
  local inactive_fg = p.inactive_fg
  local dim_fg = p.dim_fg

  -- タブの丸括弧（左）
  local elements = {
    { Background = { Color = edge_background } },
    { Foreground = { Color = tab_bg } },
    { Text = SOLID_LEFT_ARROW },
  }

  -- 各ペインのタイトルを表示
  local pane_count = #tab.panes
  -- ペインごとの幅を制限
  local pane_max = math.floor((max_width - pane_count + 1) / pane_count)
  if pane_max < 10 then pane_max = 10 end
  for i, pane in ipairs(tab.panes) do
    local pane_title = wezterm.truncate_right(pane.title, pane_max)

    local fg
    if pane.pane_id == tab.active_pane.pane_id then
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
-- マウス設定
----------------------------------------------------
-- 通常左クリック: 選択範囲があれば確定、なければハイパーリンクを開く
-- mouse_reporting=true 版はアプリ(reedline等)がマウスを掴んでいる時用。
-- これがないと SHIFT+クリックでしか開けない (WezTermのbypass_mouse_reporting_modifiers仕様)。
config.mouse_bindings = {
  {
    event = { Up = { streak = 1, button = "Left" } },
    mods = "NONE",
    mouse_reporting = false,
    action = wezterm.action_callback(function(window, pane)
      if window:get_selection_text_for_pane(pane) ~= "" then
        window:perform_action(wezterm.action.CompleteSelection("ClipboardAndPrimarySelection"), pane)
      else
        window:perform_action(wezterm.action.OpenLinkAtMouseCursor, pane)
      end
    end),
  },
  {
    event = { Up = { streak = 1, button = "Left" } },
    mods = "NONE",
    mouse_reporting = true,
    action = wezterm.action.OpenLinkAtMouseCursor,
  },
}

----------------------------------------------------
-- keybinds
----------------------------------------------------
config.disable_default_key_bindings = true
config.keys = require("keybinds").keys
config.key_tables = require("keybinds").key_tables

config.wsl_domains = {}

----------------------------------------------------
-- セッション復元 (Alt+Shift+R)
----------------------------------------------------
-- 30 秒ごとに窓・タブ・ペイン配置を保存。Claude Code のペインは nushell 上で dotcli の `c r <id>` を実行して再開する
-- (nu -e は実行後もシェルに残るので、Claude を終えてもペインは閉じない)
-- WezTerm に渡した cwd が無視されることがあるため、どのペインも nushell で先に cd する
local session_restore = require("session_restore")
session_restore.setup({
  pane_args = function(cwd, session_id, delay_ms)
    local cmds = {}
    if cwd then table.insert(cmds, string.format("cd r#'%s'#", cwd)) end
    if session_id then
      table.insert(cmds, string.format("sleep %dms", delay_ms))
      table.insert(cmds, "c r " .. session_id)
    end
    if #cmds == 0 then return nil end
    return { "nu", "-e", table.concat(cmds, "; ") }
  end,
  retry_command = function(session_id)
    return "c r " .. session_id
  end,
})

----------------------------------------------------
-- ステータス (update-status のハンドラはここだけに置く)
----------------------------------------------------
wezterm.on("update-status", function(window, pane)
  -- 右ステータスに有効なキーテーブルを出す (Alt+S の resize_pane 等)
  local name = window:active_key_table()
  window:set_right_status(name and ("TABLE: " .. name) or "")
  -- セッション復元の後処理 (復元で開いた窓の最大化、会話を再開できたかの確認)
  session_restore.on_update_status(window)
end)

return config