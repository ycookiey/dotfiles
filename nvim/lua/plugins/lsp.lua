return {
  {
    "neovim/nvim-lspconfig",
    event = { "BufReadPre", "BufNewFile" },
    dependencies = {
      "hrsh7th/nvim-cmp",
      "hrsh7th/cmp-nvim-lsp",
      "hrsh7th/cmp-buffer",
      "hrsh7th/cmp-path",
      "L3MON4D3/LuaSnip",
      "saadparwaiz1/cmp_luasnip",
    },
    config = function()
      local capabilities = require("cmp_nvim_lsp").default_capabilities()

      -- 全 LSP 共通: nvim-cmp の補完 capability を付与
      vim.lsp.config("*", { capabilities = capabilities })

      -- LSPキーマップ（LSP接続時のみ有効）
      vim.api.nvim_create_autocmd("LspAttach", {
        callback = function(ev)
          local opts = { buffer = ev.buf }
          vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
          vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)
          vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
          vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
          vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
        end,
      })

      -- Python: basedpyright（補完・型・定義ジャンプ）+ ruff（lint・整形・import整理）
      -- どちらも uv tool で導入: uv tool install basedpyright ruff
      -- nvim-lspconfig がサーバーのデフォルト(cmd/root/filetypes)を vim.lsp.config に登録するので、ここでは上書き設定のみ
      vim.lsp.config("basedpyright", {
        settings = {
          basedpyright = {
            analysis = {
              -- 競プロ向け: 本質的なエラーのみ。strict すぎる警告を抑制
              typeCheckingMode = "basic",
              diagnosticMode = "openFilesOnly",
              autoImportCompletions = true,
            },
          },
        },
      })
      vim.lsp.config("ruff", {
        on_attach = function(client)
          -- hover は basedpyright に任せ、衝突を避ける
          client.server_capabilities.hoverProvider = false
        end,
      })
      vim.lsp.enable({ "basedpyright", "ruff" })

      -- 補完
      local cmp = require("cmp")
      local luasnip = require("luasnip")
      cmp.setup({
        snippet = {
          expand = function(args) luasnip.lsp_expand(args.body) end,
        },
        mapping = cmp.mapping.preset.insert({
          ["<C-Space>"] = cmp.mapping.complete(),
          ["<CR>"] = cmp.mapping.confirm({ select = true }),
          ["<Tab>"] = cmp.mapping(function(fallback)
            if cmp.visible() then cmp.select_next_item()
            elseif luasnip.expand_or_jumpable() then luasnip.expand_or_jump()
            else fallback() end
          end, { "i", "s" }),
          ["<S-Tab>"] = cmp.mapping(function(fallback)
            if cmp.visible() then cmp.select_prev_item()
            elseif luasnip.jumpable(-1) then luasnip.jump(-1)
            else fallback() end
          end, { "i", "s" }),
        }),
        sources = cmp.config.sources({
          { name = "nvim_lsp" },
          { name = "luasnip" },
        }, {
          { name = "buffer" },
          { name = "path" },
        }),
      })

      -- 保存時に ruff でフォーマット（Python）
      vim.api.nvim_create_autocmd("BufWritePre", {
        pattern = "*.py",
        callback = function(args)
          vim.lsp.buf.format({
            bufnr = args.buf,
            async = false,
            filter = function(c) return c.name == "ruff" end,
          })
        end,
      })
    end,
  },
}
