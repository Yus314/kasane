window.BENCHMARK_DATA = {
  "lastUpdate": 1773393162339,
  "repoUrl": "https://github.com/Yus314/kasane",
  "entries": {
    "Kasane Rendering Pipeline": [
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "3bd19b74716f0e631c63287252744b6593f9cb38",
          "message": "feat: complete Phase 3 — mouse drag, clipboard, smooth scroll\n\n- Mouse drag support: DragState tracking, selection-during-scroll\n  (R-046), right-drag selection extension (R-047)\n- Clipboard integration: arboard via RenderBackend trait, bracketed\n  paste detection, paste-to-keys conversion\n- Scroll improvements: smooth scrolling with 60fps animation ticks,\n  PageUp/PageDown intercept as Scroll commands\n- Config: ClipboardConfig, MouseConfig, ScrollConfig extensions\n  (smooth, inertia)\n- 21 new tests (289 total), clippy clean\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-07T17:38:53+09:00",
          "tree_id": "06026ab8305b8b105cc7ef651462e817f2e61816",
          "url": "https://github.com/Yus314/kasane/commit/3bd19b74716f0e631c63287252744b6593f9cb38"
        },
        "date": 1772887303976,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 270,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2232,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 386,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 19669,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 76053,
            "range": "± 201",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25252,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12154,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 30,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 77,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 132,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 186,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 823,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1520,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 36187,
            "range": "± 119",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45393,
            "range": "± 222",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 45788,
            "range": "± 268",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 46167,
            "range": "± 2390",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 46035,
            "range": "± 381",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "20ad397cbf4f3e0283756b53ac57cfe2759a0f11",
          "message": "feat: add GPU backend (kasane-gui) with winit + wgpu + glyphon\n\nImplement the GUI backend for `kasane --ui gui`, enabling native window\nrendering via GPU. This completes the core of Phase G1 (MVP) and G2\n(input extensions) from the GUI backend plan.\n\nKey components:\n- kasane-gui crate: winit 0.30 event loop, wgpu 28 GPU rendering,\n  glyphon 0.10 text rendering with cosmic-text\n- CellRenderer: background quad pipeline (custom WGSL shader) +\n  glyphon text pass + cursor overlay (Block/Bar/Underline/Outline)\n- Input conversion: winit keyboard/mouse/IME → kasane InputEvent,\n  with correct Shift modifier handling for Kakoune compatibility\n- ColorResolver: maps Color::Default/Named to concrete RGB for GUI\n  (no terminal fallback available)\n- GuiBackend: RenderBackend impl for size/cursor/clipboard (arboard)\n- Config extensions: WindowConfig, FontConfig, ColorsConfig sections\n- CLI: --ui gui/tui flag with feature-gated compilation\n- Performance: opt-level=2 for deps in dev profile, row-level dirty\n  tracking (hash-based) to skip unchanged text shaping, persistent\n  GPU buffers, Basic shaping (no HarfBuzz), cached glyphon buffers\n- process.rs: Write/BufRead impls for KakouneWriter/KakouneReader\n- Nix flake: vulkan-loader, wayland, libxkbcommon, fontconfig deps\n- CI: cargo build --features gui step\n- Docs: updated architecture, roadmap, requirements, decisions\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-07T22:00:36+09:00",
          "tree_id": "3a34eb1397ceaf6bea55ef2008b13c9c50fdafa8",
          "url": "https://github.com/Yus314/kasane/commit/20ad397cbf4f3e0283756b53ac57cfe2759a0f11"
        },
        "date": 1772888783592,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 255,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2242,
            "range": "± 23",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 368,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 18977,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 74437,
            "range": "± 383",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 23471,
            "range": "± 372",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12005,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 30,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 76,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 130,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 190,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 789,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1696,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 35199,
            "range": "± 199",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 44493,
            "range": "± 743",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 45262,
            "range": "± 250",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 45296,
            "range": "± 262",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 45805,
            "range": "± 943",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "489d24f0983b9ab9041f44043ba9b117a6748543",
          "message": "fix: use configured font family instead of hardcoded Monospace\n\nThe font family from FontConfig was ignored — glyphon Attrs always used\nFamily::Monospace. Now the configured family is stored in CellRenderer\nand applied to both cell metrics measurement and text rendering.\n\nGeneric CSS family names (\"monospace\", \"serif\", \"sans-serif\", etc.) are\nmapped to glyphon's Family enum variants via gpu::to_family(), while\nspecific font names (e.g. \"JetBrains Mono\") use Family::Name(). This\npreserves correct cross-platform font resolution for the default\n\"monospace\" value while enabling user-specified fonts.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-07T23:36:34+09:00",
          "tree_id": "ad8f5a9c923b710fd95e47a27a4abbdaa7252433",
          "url": "https://github.com/Yus314/kasane/commit/489d24f0983b9ab9041f44043ba9b117a6748543"
        },
        "date": 1772894560029,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 285,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2328,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 368,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 18907,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 75805,
            "range": "± 465",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 23107,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 11938,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 30,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 75,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 128,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 192,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 819,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1549,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 35174,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 44676,
            "range": "± 429",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 45189,
            "range": "± 491",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 45415,
            "range": "± 225",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 45155,
            "range": "± 431",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "c1dda7cac9b1368a6f2ad5e9709f1181764846bc",
          "message": "docs: add ADR-011 CLI design decision\n\nDocument the drop-in replacement strategy for kak, including\nexec delegation for non-UI flags and -- separator for flag parsing.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-08T00:13:17+09:00",
          "tree_id": "c8e4c277d8c89bf7d5ea060bd7b49f1510a2a9e8",
          "url": "https://github.com/Yus314/kasane/commit/c1dda7cac9b1368a6f2ad5e9709f1181764846bc"
        },
        "date": 1772896747450,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 280,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2267,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 374,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 19711,
            "range": "± 174",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 75149,
            "range": "± 740",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 23211,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12403,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 31,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 75,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 130,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 197,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 792,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1546,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 36269,
            "range": "± 448",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45515,
            "range": "± 283",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 46627,
            "range": "± 218",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 46753,
            "range": "± 303",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 47118,
            "range": "± 243",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "2adea53e5b9887d02a46ee26d37fd7b696a77221",
          "message": "docs: add configuration reference\n\nDocument all 11 config sections (~50 options) with types, defaults,\ndescriptions, and TOML examples. Includes detailed face syntax\nreference (colors, attributes) and GUI-specific sections.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-08T10:04:05+09:00",
          "tree_id": "442e6428f3896037963e7e1d0c9b2dec36198b68",
          "url": "https://github.com/Yus314/kasane/commit/2adea53e5b9887d02a46ee26d37fd7b696a77221"
        },
        "date": 1772932177395,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 257,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2229,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 370,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 19541,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 75247,
            "range": "± 342",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 24567,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12400,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 30,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 75,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 128,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 194,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 802,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1572,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 36181,
            "range": "± 369",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45254,
            "range": "± 552",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 46388,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 46537,
            "range": "± 524",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 47016,
            "range": "± 288",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "b695d1cc9a5698b75050a3911ee495e5bec8983a",
          "message": "feat: add fullscreen and maximized window settings with F11 toggle\n\nAdd fullscreen (borderless) and maximized options to [window] config,\napply them at window creation, and intercept F11 to toggle fullscreen\nat runtime.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-08T10:19:36+09:00",
          "tree_id": "6bc9e848745f141ff4e975c69573050fff3a5dc2",
          "url": "https://github.com/Yus314/kasane/commit/b695d1cc9a5698b75050a3911ee495e5bec8983a"
        },
        "date": 1772933109214,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 261,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2261,
            "range": "± 227",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 373,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 18826,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 73208,
            "range": "± 440",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 23115,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12401,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 29,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 75,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 137,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 195,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 788,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1531,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 35622,
            "range": "± 189",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 44893,
            "range": "± 244",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 45695,
            "range": "± 239",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 45749,
            "range": "± 276",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 46052,
            "range": "± 366",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "1744d692a987489c226ae6480e9c850b31b4e30d",
          "message": "refactor: replace cursor drawing magic numbers with named constants\n\nAdd CURSOR_BAR_WIDTH, CURSOR_UNDERLINE_HEIGHT, and CURSOR_OUTLINE_THICKNESS\nconstants to make cursor rendering dimensions self-documenting.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-08T11:50:05+09:00",
          "tree_id": "0aca19aad2a303fba4a7b45b67b71523ec7c269f",
          "url": "https://github.com/Yus314/kasane/commit/1744d692a987489c226ae6480e9c850b31b4e30d"
        },
        "date": 1772938843351,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 256,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2256,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 369,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 19208,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 73951,
            "range": "± 412",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25587,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 11995,
            "range": "± 69",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 29,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 76,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 128,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 193,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 774,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1641,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 34929,
            "range": "± 118",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 44768,
            "range": "± 288",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 44886,
            "range": "± 265",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 45028,
            "range": "± 367",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 45495,
            "range": "± 344",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 57303,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 550839,
            "range": "± 7199",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2756186,
            "range": "± 11272",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2788,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/set_cursor",
            "value": 790,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 53747,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 8715,
            "range": "± 133",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 33795,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 142996,
            "range": "± 1129",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 4388,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/set_cursor",
            "value": 4290,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 10396,
            "range": "± 68",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 35009,
            "range": "± 572",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 170268,
            "range": "± 445",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 316009,
            "range": "± 792",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2820430,
            "range": "± 23653",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6445412,
            "range": "± 85746",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12016,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 72997,
            "range": "± 472",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 145209,
            "range": "± 1184",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "719519a9f76efa6611352561c06e3f7c79f6198b",
          "message": "fix(ci): update iai-callgrind-runner to 0.14.2 to match library version\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T00:00:25+09:00",
          "tree_id": "46337ebcc1f152bab1600aa65edfac5b7372e160",
          "url": "https://github.com/Yus314/kasane/commit/719519a9f76efa6611352561c06e3f7c79f6198b"
        },
        "date": 1772983210927,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 247,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2235,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 357,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 29256,
            "range": "± 215",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 100111,
            "range": "± 617",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 34265,
            "range": "± 213",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25797,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13511,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3323,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20851,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 30,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 76,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 134,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 189,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 974,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1775,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 47343,
            "range": "± 313",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 48960,
            "range": "± 780",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 59138,
            "range": "± 587",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 59374,
            "range": "± 445",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 59375,
            "range": "± 456",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 44244,
            "range": "± 270",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 46119,
            "range": "± 225",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 49077,
            "range": "± 614",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65523,
            "range": "± 813",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 571148,
            "range": "± 14125",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2792331,
            "range": "± 48276",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3101,
            "range": "± 64",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/set_cursor",
            "value": 818,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 59493,
            "range": "± 1220",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 1499,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 4033,
            "range": "± 546",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 17177,
            "range": "± 2077",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 743,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/set_cursor",
            "value": 681,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 3597,
            "range": "± 143",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 47236,
            "range": "± 251",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 205656,
            "range": "± 1635",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 368488,
            "range": "± 1372",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2796261,
            "range": "± 14569",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5523170,
            "range": "± 26515",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13547,
            "range": "± 141",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 82067,
            "range": "± 611",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 163378,
            "range": "± 1287",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 157254,
            "range": "± 2103",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 899084,
            "range": "± 9305",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2161,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 148029,
            "range": "± 2673",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 182127,
            "range": "± 3764",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 151627,
            "range": "± 4500",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4585806,
            "range": "± 17979",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18057398,
            "range": "± 55113",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2127161,
            "range": "± 9784",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20430566,
            "range": "± 168293",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6920,
            "range": "± 144",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 56838,
            "range": "± 312",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 654,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7843,
            "range": "± 39",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "1e2444a608506de032249abb282f23e0637d8b0f",
          "message": "refactor: reduce duplication and tighten visibility across codebase\n\n- Use Element::container() constructor instead of struct literals (8 sites)\n- Extract build_menu_item_element() helper to deduplicate menu item construction\n- Inline trivial compute_visible_lines() passthrough\n- Consolidate duplicated test helpers (default_state, root_area, make_line)\n  into shared kasane-core/src/test_utils.rs module\n- Delegate test_helpers draw_shadow/draw_border to paint.rs implementations\n- Remove dead CellRenderer struct (~360 lines) and deduplicate cursor constants\n  into gpu/mod.rs\n- Restrict unnecessary pub visibility in kasane-tui and BgPipeline fields\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T10:32:17+09:00",
          "tree_id": "857ea227f6611dc8c1694f7105d000e9d8735a0d",
          "url": "https://github.com/Yus314/kasane/commit/1e2444a608506de032249abb282f23e0637d8b0f"
        },
        "date": 1773021291578,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 246,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2345,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 341,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28793,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 98726,
            "range": "± 320",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 33641,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25150,
            "range": "± 120",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12476,
            "range": "± 178",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3300,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20788,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 37,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 76,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 131,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 195,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 869,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1550,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 45253,
            "range": "± 297",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 46783,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 56511,
            "range": "± 299",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 56746,
            "range": "± 360",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 56783,
            "range": "± 538",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 42967,
            "range": "± 211",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 44810,
            "range": "± 249",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 47537,
            "range": "± 295",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 64881,
            "range": "± 1293",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 563256,
            "range": "± 13312",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2766102,
            "range": "± 28276",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2907,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/set_cursor",
            "value": 824,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 58527,
            "range": "± 2114",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 1223,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 3128,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 13626,
            "range": "± 381",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 621,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/set_cursor",
            "value": 578,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 3365,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 45277,
            "range": "± 110",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 196979,
            "range": "± 423",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 353262,
            "range": "± 1514",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2769511,
            "range": "± 7519",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5544823,
            "range": "± 25445",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12289,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 74582,
            "range": "± 154",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 147680,
            "range": "± 840",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148242,
            "range": "± 1731",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 856774,
            "range": "± 9238",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2093,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 142138,
            "range": "± 1091",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 180582,
            "range": "± 3612",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 152038,
            "range": "± 4030",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4468766,
            "range": "± 8301",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18194255,
            "range": "± 47046",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2098344,
            "range": "± 3745",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20165738,
            "range": "± 69337",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7488,
            "range": "± 304",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55543,
            "range": "± 549",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 635,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7447,
            "range": "± 19",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "288bab51ba91647efd7ed9dfec441f2a642b24a9",
          "message": "docs: rewrite performance.md in English with measured benchmark data\n\nTranslate performance.md from Japanese to English and fill all blank\nmeasurement fields with actual benchmark results. Key updates:\n- All 14 micro, 8 integration, 20 extended benchmarks measured\n- TUI backend, replay, GPU CPU-side, and E2E pipeline benchmarks added\n- Allocation breakdown per phase from alloc_budget\n- Latency distribution (p50-max) from HDR histogram\n- ADR-010 evaluation against real data (verdict: premature)\n- Corrected clear+paint accounting (separated in breakdown)\n- Added performance tasks to Phase 4a in roadmap.md\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T12:46:13+09:00",
          "tree_id": "2c9ff8a4349e943b061dd8b64c4637bd73423191",
          "url": "https://github.com/Yus314/kasane/commit/288bab51ba91647efd7ed9dfec441f2a642b24a9"
        },
        "date": 1773029201643,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 467,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2827,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 353,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 27357,
            "range": "± 193",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 93535,
            "range": "± 6775",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 31700,
            "range": "± 1559",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 23551,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12634,
            "range": "± 261",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3254,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20276,
            "range": "± 958",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 34,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 94,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 155,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 212,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 774,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1561,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 44575,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 47318,
            "range": "± 326",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 57588,
            "range": "± 352",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 58121,
            "range": "± 1333",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 58454,
            "range": "± 1107",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 41589,
            "range": "± 320",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 43938,
            "range": "± 255",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 47640,
            "range": "± 276",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 67634,
            "range": "± 614",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 593594,
            "range": "± 15764",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2947352,
            "range": "± 72406",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3165,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/set_cursor",
            "value": 797,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 62123,
            "range": "± 2732",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 1277,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 3209,
            "range": "± 184",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 13700,
            "range": "± 605",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 640,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/set_cursor",
            "value": 587,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 3489,
            "range": "± 210",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 44747,
            "range": "± 685",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 196281,
            "range": "± 4019",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 351218,
            "range": "± 5892",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2953257,
            "range": "± 9140",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5868506,
            "range": "± 60341",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12937,
            "range": "± 139",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81655,
            "range": "± 396",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 156520,
            "range": "± 3208",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 4131,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 3222,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 166200,
            "range": "± 1299",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 851942,
            "range": "± 17353",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2143,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 143189,
            "range": "± 2793",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 184836,
            "range": "± 5362",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 153643,
            "range": "± 4652",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4553623,
            "range": "± 18851",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18487897,
            "range": "± 391321",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2147964,
            "range": "± 25112",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20476869,
            "range": "± 151052",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7560,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55516,
            "range": "± 789",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 626,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8225,
            "range": "± 49",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "6c86ce3206a5409b123c71184ab8469e9fbf1f58",
          "message": "feat(macros): add verified DirtyFlags dependency tracking (ADR-010 stage 2)\n\nWalk component function bodies with syn::visit to detect state.field\naccesses, map them to DirtyFlags via FIELD_FLAG_MAP, and emit compile\nerrors when deps() doesn't cover observed reads. Adds allow(field, ...)\nescape hatch for intentional gaps and scans macro token streams for\nfield accesses inside format!/println! etc.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T13:13:31+09:00",
          "tree_id": "25b39981484ff899697fb9a8d649fa659e593db8",
          "url": "https://github.com/Yus314/kasane/commit/6c86ce3206a5409b123c71184ab8469e9fbf1f58"
        },
        "date": 1773030801438,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 431,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 2879,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 329,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28943,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 99033,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 34605,
            "range": "± 1171",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25154,
            "range": "± 70",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13418,
            "range": "± 429",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3313,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20809,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 32,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 78,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 130,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 192,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 895,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1516,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 46149,
            "range": "± 201",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 48720,
            "range": "± 672",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 59502,
            "range": "± 488",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 60144,
            "range": "± 470",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 60144,
            "range": "± 302",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 43434,
            "range": "± 459",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 45915,
            "range": "± 176",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 48344,
            "range": "± 482",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 66861,
            "range": "± 879",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 583479,
            "range": "± 16447",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2847990,
            "range": "± 32514",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2978,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/set_cursor",
            "value": 785,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 59634,
            "range": "± 707",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 1319,
            "range": "± 175",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 3498,
            "range": "± 331",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 14895,
            "range": "± 1287",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 650,
            "range": "± 114",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/set_cursor",
            "value": 586,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 3376,
            "range": "± 281",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46321,
            "range": "± 800",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 201083,
            "range": "± 2625",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 360004,
            "range": "± 1064",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2896010,
            "range": "± 15303",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5763504,
            "range": "± 32288",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12750,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81610,
            "range": "± 1337",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 161890,
            "range": "± 3775",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 4057,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 3268,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150831,
            "range": "± 3702",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 892825,
            "range": "± 3630",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2128,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 142049,
            "range": "± 365",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 182092,
            "range": "± 3470",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 151720,
            "range": "± 3457",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4521474,
            "range": "± 12548",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 17851508,
            "range": "± 129406",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2131382,
            "range": "± 4776",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20254139,
            "range": "± 37442",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7958,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55756,
            "range": "± 5718",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 597,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8235,
            "range": "± 16",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "ae0e718eb393cf069bb5fd9ec5609dcba7ba0e80",
          "message": "feat(render): add SceneCache for DrawCommand-level caching (ADR-010 stage 3)\n\nCache Vec<DrawCommand> per view section (base, menu, info) so that\ncursor animation frames reuse cached commands with zero pipeline work,\nand menu navigation only repaints the menu section.\n\n- SceneCache: per-section invalidation mirroring ViewCache\n- scene_render_pipeline_scene_cached(): returns &[DrawCommand] from cache\n- ViewSections + view_sections_cached(): decomposed view for per-section processing\n- layout_overlay(): reusable single-overlay layout helper\n- GUI cursor animation: cursor_dirty flag replaces DirtyFlags::BUFFER hack\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T13:47:12+09:00",
          "tree_id": "b926cf807799f29f3b1fc25d9a2d4444e7d3557f",
          "url": "https://github.com/Yus314/kasane/commit/ae0e718eb393cf069bb5fd9ec5609dcba7ba0e80"
        },
        "date": 1773032312475,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 160190,
            "range": "± 5327",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 880217,
            "range": "± 41219",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2066,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 159618,
            "range": "± 1062",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 186905,
            "range": "± 11895",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 155329,
            "range": "± 5090",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4733276,
            "range": "± 50595",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18686303,
            "range": "± 56571",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2224464,
            "range": "± 12875",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 21232949,
            "range": "± 403137",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6926,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55555,
            "range": "± 1338",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 670,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8321,
            "range": "± 93",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "221e2c941c7a93da665203c6605a6bedf4be9bfe",
          "message": "feat(render): add compiled paint patches for fast-path rendering (ADR-010 stage 4)\n\nImplement PaintPatch trait and three built-in patches that bypass the\nfull Element tree → layout → paint pipeline for common dirty states:\n\n- StatusBarPatch: repaints ~80 cells when only STATUS is dirty\n- MenuSelectionPatch: swaps faces on old/new selected items (~10 cells)\n- CursorPatch: updates 2 cells when only cursor position changed\n\nAlso adds ComponentCache<T> generic memoization wrapper, LayoutCache for\nsection-level repaint positions, render_pipeline_sectioned() for partial\nrepaints, and render_pipeline_patched() with debug correctness assertions.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T14:17:01+09:00",
          "tree_id": "34c785b84a0a38d0b0a2569637e854c3446c5519",
          "url": "https://github.com/Yus314/kasane/commit/221e2c941c7a93da665203c6605a6bedf4be9bfe"
        },
        "date": 1773034118736,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 146569,
            "range": "± 1983",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 957749,
            "range": "± 10646",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2051,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 147200,
            "range": "± 924",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 185002,
            "range": "± 3440",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 153044,
            "range": "± 4652",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4539145,
            "range": "± 8814",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18305245,
            "range": "± 42213",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2137104,
            "range": "± 6356",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20505898,
            "range": "± 171219",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6894,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55560,
            "range": "± 219",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 626,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8161,
            "range": "± 114",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "ca6f173fd4345b6bd85b29622b01ace9c54eb5ed",
          "message": "docs: update roadmap, architecture, decisions to reflect ADR-010 completion\n\nMark ADR-010 stages 1-4 as completed in roadmap. Add missing source files\n(patch.rs, state/info.rs, state/menu.rs, test_utils.rs, test_helpers/) to\narchitecture source tree. Update rendering pipeline with caching layers\n(ViewCache, LayoutCache, SceneCache, PaintPatch). Add implementation record\nto ADR-010 in decisions.md.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T14:37:20+09:00",
          "tree_id": "d5767f20e013d2379217b955902c590e8540712e",
          "url": "https://github.com/Yus314/kasane/commit/ca6f173fd4345b6bd85b29622b01ace9c54eb5ed"
        },
        "date": 1773035325815,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 142654,
            "range": "± 5363",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 825738,
            "range": "± 4197",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2043,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 136680,
            "range": "± 1071",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 182019,
            "range": "± 5134",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 151304,
            "range": "± 3690",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4562255,
            "range": "± 14721",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 18058217,
            "range": "± 73496",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 2142210,
            "range": "± 13717",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20444286,
            "range": "± 83785",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 8029,
            "range": "± 610",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55725,
            "range": "± 311",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 598,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7462,
            "range": "± 13",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "d734bc1ab8be40ed54f73729a73f57cfd2132829",
          "message": "perf(render): add line-level dirty tracking for incremental buffer repaints\n\nCompare old vs new lines in apply(Draw) to identify which lines actually\nchanged. When only BUFFER is dirty and some lines are clean, skip\ngrid.clear() and paint only dirty lines. swap_with_dirty() copies only\ndirty rows to previous, preserving clean row content for the next frame.\n\nThis reduces CPU work for single-character edits from ~49 μs (full\n1,920-cell repaint) to ~7 μs (only changed line's cells).\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T15:01:29+09:00",
          "tree_id": "e0ab0878c7bad8405edb0b26ceb7a43bfd828a13",
          "url": "https://github.com/Yus314/kasane/commit/d734bc1ab8be40ed54f73729a73f57cfd2132829"
        },
        "date": 1773036750993,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150190,
            "range": "± 4919",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 831491,
            "range": "± 10397",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2040,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 137168,
            "range": "± 1808",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 157981,
            "range": "± 5415",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 123711,
            "range": "± 3770",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3362283,
            "range": "± 43531",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15596539,
            "range": "± 78848",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1737324,
            "range": "± 5442",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16144464,
            "range": "± 94364",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6854,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55182,
            "range": "± 224",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 614,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7641,
            "range": "± 18",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "07b588da8e21f26450c96c1186c77bddb6552530",
          "message": "refactor(layout): unify overlay positioning into layout/position.rs\n\nExtract shared layout_single_overlay() from the duplicated logic in\nflex::place_stack and render::layout_overlay. Both call sites now\ndelegate to the single canonical implementation.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T15:34:58+09:00",
          "tree_id": "0ba34b11c4253189423696f9cc33dd7b2ee16ae5",
          "url": "https://github.com/Yus314/kasane/commit/07b588da8e21f26450c96c1186c77bddb6552530"
        },
        "date": 1773038782555,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150108,
            "range": "± 357",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 874710,
            "range": "± 14021",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2149,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 140344,
            "range": "± 692",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 157785,
            "range": "± 4898",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122517,
            "range": "± 3778",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3415003,
            "range": "± 24355",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15727582,
            "range": "± 64688",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1768509,
            "range": "± 6380",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16324264,
            "range": "± 69440",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6837,
            "range": "± 268",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55338,
            "range": "± 3599",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 634,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7643,
            "range": "± 16",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "d64e39dab027c86e2b84156b7ff9906648909497",
          "message": "feat(render): two-column inline completion menu\n\nRender inline completion menus with separate candidate and docstring\ncolumns, eliminating the excessive padding that Kakoune inserts for\nalignment. Candidate column is capped at 40% of screen width; longer\ncandidates are truncated with \"…\".\n\n- truncate_atoms(): grapheme-aware truncation with ellipsis\n- build_split_item_element(): flat StyledLine with candidate+gap+doc\n- build_menu_inline: uses effective_content_width for narrower menus\n- build_replacement_menu_overlay / get_menu_rect: use effective width\n- MenuSelectionPatch: disabled for two-column menus (sectioned repaint\n  handles them efficiently)\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T16:08:25+09:00",
          "tree_id": "ba0ec96948592d8a4d3234dd4753d026bec9b9dc",
          "url": "https://github.com/Yus314/kasane/commit/d64e39dab027c86e2b84156b7ff9906648909497"
        },
        "date": 1773040816663,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150915,
            "range": "± 2930",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 882892,
            "range": "± 24228",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2119,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144561,
            "range": "± 1709",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156658,
            "range": "± 7239",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 121801,
            "range": "± 3735",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3528755,
            "range": "± 8434",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15830088,
            "range": "± 55621",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1797083,
            "range": "± 5518",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16613891,
            "range": "± 100442",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7408,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55100,
            "range": "± 704",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 637,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7659,
            "range": "± 35",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "970158660c5263f47d52100facd5d5d7b2740832",
          "message": "refactor(core): restructure modules for maintainability\n\n- Split render/tests.rs (1113 lines, 36 tests) into tests/{cursor,pipeline,view_cache,scene_cache}.rs\n- Split state/tests.rs (786 lines, 38 tests) into tests/{apply,update,input,dirty_flags}.rs\n- Extract render/pipeline.rs (pipeline functions) and render/cursor.rs from render/mod.rs\n- Split render/scene.rs into scene/{mod,cache}.rs (SceneCache separation)\n- Consolidate test utilities into test_support.rs (shared across unit + integration tests)\n- Restrict Theme and ComponentCache visibility to pub(crate)\n- Introduce FlexMeasure and ContainerStyle structs to eliminate #[allow(clippy::too_many_arguments)]\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T16:44:29+09:00",
          "tree_id": "3a66e34ba1521ecbf0bc34b1586a532192684a0e",
          "url": "https://github.com/Yus314/kasane/commit/970158660c5263f47d52100facd5d5d7b2740832"
        },
        "date": 1773042953241,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 143793,
            "range": "± 2152",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 833775,
            "range": "± 3988",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2156,
            "range": "± 133",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 137445,
            "range": "± 372",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156526,
            "range": "± 5239",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 121334,
            "range": "± 3550",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3486492,
            "range": "± 404299",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16156120,
            "range": "± 61845",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1802489,
            "range": "± 43775",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16949089,
            "range": "± 110116",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7409,
            "range": "± 225",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 59864,
            "range": "± 356",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 612,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7441,
            "range": "± 38",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "76a01d3ea53b1aff9be042aff6c9749ab246b31c",
          "message": "feat(plugin): fix three critical blockers for real plugin usage\n\n1. Command::RequestRedraw(DirtyFlags): plugins can now trigger redraws\n   by returning RequestRedraw commands. extract_redraw_flags() merges\n   flags before returning from update().\n\n2. Plugin::contribute_overlay(): plugins can specify overlay positioning\n   via OverlayAnchor (AnchorPoint/Absolute) instead of being forced to\n   full-screen. Legacy Slot::Overlay is preserved with backward-compat\n   wrapping. PluginRegistry::collect_overlays() replaces inline code in\n   view_sections_cached().\n\n3. Mouse event routing via HitMap: flat precomputed map of interactive\n   regions, stored on PluginRegistry, rebuilt after each render frame.\n   Mouse events are hit-tested and routed to plugins before Kakoune\n   forwarding. Integrated in both TUI and GUI backends.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T20:31:15+09:00",
          "tree_id": "e40f0fabcf9d501d0de3fd62bc3ddd89036d969f",
          "url": "https://github.com/Yus314/kasane/commit/76a01d3ea53b1aff9be042aff6c9749ab246b31c"
        },
        "date": 1773056536595,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148327,
            "range": "± 2917",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 838648,
            "range": "± 2423",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2005,
            "range": "± 172",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 137275,
            "range": "± 596",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 160554,
            "range": "± 5630",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 125233,
            "range": "± 3459",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3406547,
            "range": "± 7110",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15756756,
            "range": "± 189569",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1757277,
            "range": "± 5082",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16306342,
            "range": "± 125972",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7974,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 59505,
            "range": "± 574",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 620,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7581,
            "range": "± 20",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "230576259ecbee1f4d703b05e92277d10f477b6f",
          "message": "feat(plugin): add lifecycle hooks, input observation, line decoration, menu transform, and command extension\n\nImplement Phase 4a plugin extensibility improvements:\n\n- Lifecycle: on_init/on_shutdown/on_state_changed hooks with PluginRegistry\n  init_all/shutdown_all orchestration\n- Input: observe_key/observe_mouse (non-consuming notification), reorder\n  dispatch so plugins can override builtin PageUp/PageDown\n- Line decoration: LineDecoration struct with left/right gutter elements\n  and per-line background override via contribute_line\n- Menu transform: transform_menu_item for per-item customization\n- Commands: ScheduleTimer, PluginMessage, SetConfig via DeferredCommand\n  extraction and recursive handling in both TUI and GUI event loops\n- AppState helpers: visible_line_range, buffer_line_count, has_menu,\n  has_info, is_prompt_mode\n- Proc macro: codegen for all new Plugin trait methods\n- Tests: 20+ new unit tests, 4 trybuild pass cases\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T21:13:26+09:00",
          "tree_id": "13d09a6dc49b5e26009ab3149ab8da932f38ae57",
          "url": "https://github.com/Yus314/kasane/commit/230576259ecbee1f4d703b05e92277d10f477b6f"
        },
        "date": 1773059067398,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148138,
            "range": "± 550",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 895069,
            "range": "± 32289",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2058,
            "range": "± 124",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 143517,
            "range": "± 1037",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 157152,
            "range": "± 5945",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 121814,
            "range": "± 3549",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3423570,
            "range": "± 16087",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15725817,
            "range": "± 94415",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1760739,
            "range": "± 6056",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16358313,
            "range": "± 181337",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7972,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55526,
            "range": "± 266",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 579,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7450,
            "range": "± 53",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "6ecf4c50d886ea0c6ec33c4420be15355098e660",
          "message": "feat(plugin): add slot cache with L1 state hash and L3 DirtyFlags auto-derivation\n\nImplement ADR-010 Stage 5 L1+L3 optimizations for plugin rendering.\nWhen build_base() is recomputed (ViewCache miss), individual plugins'\ncontribute() calls are now skipped if:\n- L1: Plugin internal state hash unchanged (detected by DefaultHasher)\n- L3: The AppState fields the slot function reads haven't changed\n  (detected by auto-derived DirtyFlags from macro body analysis)\n\nKey changes:\n- Extract shared StateFieldVisitor/FIELD_FLAG_MAP into analysis.rs\n- Add Slot::index()/COUNT/ALL_VARIANTS for array-based cache indexing\n- Add Plugin::state_hash() and Plugin::slot_deps() trait methods\n- Add PluginSlotCache with RefCell in PluginRegistry\n- prepare_plugin_cache() two-level invalidation before each frame\n- #[kasane::plugin] macro: derive(Hash) on #[state], auto-derive slot_deps\n- Integration in both TUI and GUI event loops\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-09T21:50:08+09:00",
          "tree_id": "af2432e3bbb127c14a51a0b5b415887b42535f4a",
          "url": "https://github.com/Yus314/kasane/commit/6ecf4c50d886ea0c6ec33c4420be15355098e660"
        },
        "date": 1773061284487,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 145310,
            "range": "± 1390",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 838797,
            "range": "± 3679",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2039,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 136641,
            "range": "± 1968",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156454,
            "range": "± 5890",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122801,
            "range": "± 5499",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3407753,
            "range": "± 31180",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15739386,
            "range": "± 50815",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1765958,
            "range": "± 21481",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16292508,
            "range": "± 363262",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7407,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55563,
            "range": "± 204",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 575,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7452,
            "range": "± 19",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "cf4e44492b6c9d938cc8454301abc07b76c291f1",
          "message": "test(plugin): add integration tests for all plugin extension points\n\nAdd kasane-core/tests/plugin_integration.rs with 7 macro-defined plugin\ntests covering the full E2E path: #[kasane_plugin] → PluginRegistry →\nview → layout → paint → CellGrid.\n\nTests: multi-extension (Slot+LineDecoration+Lifecycle), Decorator,\nReplacement, Overlay, handle_key first-wins, Event/Message delivery,\nand MenuTransform.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T00:06:59+09:00",
          "tree_id": "f3dcecbd4818ff084f9648eff5c4fe54bb2d491c",
          "url": "https://github.com/Yus314/kasane/commit/cf4e44492b6c9d938cc8454301abc07b76c291f1"
        },
        "date": 1773069485286,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 152718,
            "range": "± 4436",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 882779,
            "range": "± 30486",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2105,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144127,
            "range": "± 566",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 155971,
            "range": "± 6048",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122646,
            "range": "± 3828",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3432056,
            "range": "± 98079",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15663601,
            "range": "± 58610",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1762640,
            "range": "± 8148",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16270194,
            "range": "± 56674",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7428,
            "range": "± 49",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55390,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 637,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7526,
            "range": "± 23",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "e5e0a0caa4851c16d9f56cb4e9dc026fadd59543",
          "message": "feat(plugin): add ColorPreviewPlugin with interactive color picker\n\nAdd a color preview plugin that detects hex colors (#RRGGBB, #RGB, rgb:RRGGBB)\nin buffer lines and provides:\n- Left gutter swatches showing detected colors\n- Floating overlay with interactive ▲/▼ arrows per RGB channel\n- Click to adjust color ±1, Ctrl/Shift+click for ±16 steps\n- Search-based buffer replacement via `exec -draft` for reliable edits\n- Registered in both TUI and GUI backends\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T14:32:16+09:00",
          "tree_id": "7a14eade9967f30e3f181931162abc600bf905aa",
          "url": "https://github.com/Yus314/kasane/commit/e5e0a0caa4851c16d9f56cb4e9dc026fadd59543"
        },
        "date": 1773121403637,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 152145,
            "range": "± 1206",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 883881,
            "range": "± 19402",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2226,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 147765,
            "range": "± 3595",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 158120,
            "range": "± 5397",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 123486,
            "range": "± 3392",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3444328,
            "range": "± 5523",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15658930,
            "range": "± 39990",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1764887,
            "range": "± 4766",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16380348,
            "range": "± 52024",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 8009,
            "range": "± 535",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55529,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 617,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7459,
            "range": "± 49",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "7d2decbf4909e60494423a9b9d52ee736885d493",
          "message": "docs: update Plugin trait, event propagation, glossary, and roadmap to reflect Phase 4a\n\n- declarative-ui.md: rewrite Plugin trait (8→18 methods), update Command enum,\n  add observe_key/observe_mouse to event propagation flows, update Phase 4 with\n  achieved CursorLinePlugin and ColorPreviewPlugin demos\n- glossary.md: add 12 plugin system terms (LineDecoration, PluginSlotCache, etc.),\n  update Command variants, add DirtyFlags\n- requirements.md: add status column to E-xxx table marking partial demos\n- roadmap.md: add completed plugin system demo section to Phase 4a\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T14:45:28+09:00",
          "tree_id": "45c5850f9e385c6a035913c4afb21a6f5004fe4c",
          "url": "https://github.com/Yus314/kasane/commit/7d2decbf4909e60494423a9b9d52ee736885d493"
        },
        "date": 1773122203854,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 157318,
            "range": "± 3841",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 866456,
            "range": "± 27992",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2239,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 148784,
            "range": "± 2608",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156468,
            "range": "± 5376",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122682,
            "range": "± 3451",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3453870,
            "range": "± 19374",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15676677,
            "range": "± 63713",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1763591,
            "range": "± 4570",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16336459,
            "range": "± 80547",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7970,
            "range": "± 265",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55499,
            "range": "± 284",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 576,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7459,
            "range": "± 24",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "4e47c786e2705de8132ae9ca7837762a02c35de6",
          "message": "docs: restructure roadmap Phase 4+, separate upstream blockers\n\n- Restructure Phase 4a/4b with detailed implementation strategies\n  and built-in vs external plugin classification criteria\n- Add Phase 5 outline (multi-pane + external plugin system)\n- Add independent performance track section\n- Create upstream-dependencies.md for Kakoune-blocked items\n  (E-002, E-020, E-021, R-062, E-001 full version)\n- Add Phase column to requirements.md extension feature table\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T15:04:17+09:00",
          "tree_id": "2cfccfa1f208684c0c97e6ce72c8b4183d3e3b33",
          "url": "https://github.com/Yus314/kasane/commit/4e47c786e2705de8132ae9ca7837762a02c35de6"
        },
        "date": 1773123343442,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 143905,
            "range": "± 6751",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 860968,
            "range": "± 36511",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2206,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 145323,
            "range": "± 1099",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156171,
            "range": "± 5231",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 121990,
            "range": "± 4157",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3451123,
            "range": "± 8189",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15611765,
            "range": "± 68937",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1765893,
            "range": "± 8363",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16380438,
            "range": "± 78381",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7434,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55657,
            "range": "± 386",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 616,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7513,
            "range": "± 85",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "3abda3f16a57d9c302c32d8ca7877e44e58c9855",
          "message": "docs: add four-layer responsibility model (ADR-012)\n\nEstablish systematic criteria for determining which layer owns a feature:\nupstream (Kakoune) / core / built-in plugin / external plugin.\nReclassify R-052 and E-040 as upstream-dependent, update Phase 4b scope.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T15:47:45+09:00",
          "tree_id": "1e932658ff46164e821ddce3ccfbee51897a1279",
          "url": "https://github.com/Yus314/kasane/commit/3abda3f16a57d9c302c32d8ca7877e44e58c9855"
        },
        "date": 1773125957140,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150874,
            "range": "± 1873",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 885864,
            "range": "± 22628",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2170,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 141859,
            "range": "± 7443",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 156458,
            "range": "± 5221",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122074,
            "range": "± 3535",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3450843,
            "range": "± 11094",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15605537,
            "range": "± 92636",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1774394,
            "range": "± 16026",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16430796,
            "range": "± 52147",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7393,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55508,
            "range": "± 163",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 618,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7461,
            "range": "± 26",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "be479f3f2844ec48afcbbc247f35d0b544deafa9",
          "message": "docs: align roadmap.md with four-layer responsibility model\n\n- Replace \"組み込み vs 外部\" criteria table with four-layer flowchart\n  reference, resolving conflict with API parity principle\n- Add upstream-dependency notes to R-052 in Phase 2 and Phase 4a lists\n- Remove E-040 (#4138) and R-052 (#2727) from Phase 4b issue categories\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T15:54:41+09:00",
          "tree_id": "e4a38af571c8d15264a9ada389ef9f4d9fbbffa2",
          "url": "https://github.com/Yus314/kasane/commit/be479f3f2844ec48afcbbc247f35d0b544deafa9"
        },
        "date": 1773126364898,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 151440,
            "range": "± 564",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 828364,
            "range": "± 14054",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2138,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 143106,
            "range": "± 619",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 158071,
            "range": "± 5233",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 122965,
            "range": "± 4730",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3457119,
            "range": "± 6358",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15728802,
            "range": "± 117716",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1770617,
            "range": "± 5535",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16303199,
            "range": "± 105396",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7956,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55507,
            "range": "± 371",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 598,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7652,
            "range": "± 92",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "cdb2321d7f386c5c7fbc12d58fe735316255ea89",
          "message": "docs: update roadmap and docs for R-050 completion\n\nMark R-050 as achieved in roadmap Phase 4a section, update\nlayer-responsibilities and requirements tables.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-10T16:30:56+09:00",
          "tree_id": "392af4a11ea4437a5a43345499f8e76111a61cd9",
          "url": "https://github.com/Yus314/kasane/commit/cdb2321d7f386c5c7fbc12d58fe735316255ea89"
        },
        "date": 1773128523108,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 151019,
            "range": "± 3200",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 887849,
            "range": "± 41663",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2110,
            "range": "± 172",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 148394,
            "range": "± 330",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 171443,
            "range": "± 5211",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 139029,
            "range": "± 3679",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3620941,
            "range": "± 6143",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 17001592,
            "range": "± 53745",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1852917,
            "range": "± 5689",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 17389869,
            "range": "± 50414",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6911,
            "range": "± 218",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55377,
            "range": "± 335",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 627,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8316,
            "range": "± 10",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "09a85aad67b387f4aab7c500cab5dc4472d24714",
          "message": "docs: add README with project philosophy and usage guide\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-11T12:51:06+09:00",
          "tree_id": "f0ea583e5c4814f986ea9f20db26b111bd28666e",
          "url": "https://github.com/Yus314/kasane/commit/09a85aad67b387f4aab7c500cab5dc4472d24714"
        },
        "date": 1773201705427,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 128901,
            "range": "± 455",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 736833,
            "range": "± 7570",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1815,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 122630,
            "range": "± 389",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 169113,
            "range": "± 4009",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 134572,
            "range": "± 2705",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3068295,
            "range": "± 29588",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16808213,
            "range": "± 54030",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1781396,
            "range": "± 5859",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 16807085,
            "range": "± 40629",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7016,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 53124,
            "range": "± 210",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 515,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 6231,
            "range": "± 24",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "e2e950baa2960aaf72fe00ecb469aff46552cac3",
          "message": "docs: revise Philosophy section to accurately reflect Kakoune's design\n\nRewrite Kakoune's description to align with mawww's stated philosophy:\nUnix composability, deliberate rejection of plugins, and json-ui as a\nsecondary benefit of client-server architecture. Remove \"Separation as\nvirtue\" bullet that misattributed design intent.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-11T13:06:43+09:00",
          "tree_id": "8f474d1624a8e556932454058bdb749ed4528298",
          "url": "https://github.com/Yus314/kasane/commit/e2e950baa2960aaf72fe00ecb469aff46552cac3"
        },
        "date": 1773202684785,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 147569,
            "range": "± 3673",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 892839,
            "range": "± 8597",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2159,
            "range": "± 128",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 146403,
            "range": "± 868",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 166004,
            "range": "± 3481",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 135467,
            "range": "± 2051",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3568173,
            "range": "± 28907",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16702454,
            "range": "± 36903",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1838982,
            "range": "± 22769",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 17069289,
            "range": "± 67071",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6917,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54805,
            "range": "± 443",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 619,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7474,
            "range": "± 37",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "74fa898c26c0e252a9aeece8a4bbef2c175cb2f7",
          "message": "docs: reduce redundancy and improve clarity in README\n\n- Consolidate ~49 µs/frame mention to Philosophy only (was in 3 places)\n- Remove duplicate gutter/fuzzy-finder examples across paragraphs\n- Remove Features items already covered by Philosophy (Declarative UI, perf)\n- Remove redundant cargo build --release from Installation\n- Reduce jargon (side-effect commands → commands)\n- Improve paragraph flow in Philosophy section\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-11T13:12:23+09:00",
          "tree_id": "a3ebb46cbd774e0dd5321e0c86a6886a61421851",
          "url": "https://github.com/Yus314/kasane/commit/74fa898c26c0e252a9aeece8a4bbef2c175cb2f7"
        },
        "date": 1773203017621,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 149462,
            "range": "± 796",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 842249,
            "range": "± 36514",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2143,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144425,
            "range": "± 3366",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_80x24",
            "value": 165369,
            "range": "± 4871",
            "unit": "ns/iter"
          },
          {
            "name": "e2e_pipeline/json_to_escape_realistic",
            "value": 135305,
            "range": "± 3580",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3572068,
            "range": "± 47575",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16706626,
            "range": "± 89599",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1837672,
            "range": "± 43383",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 17049946,
            "range": "± 54182",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6894,
            "range": "± 242",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54791,
            "range": "± 165",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 620,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7472,
            "range": "± 24",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "13ba03721f2709b9250fa7c32a62e5b9c842c040",
          "message": "feat(wasm): add auto-discovery for WASM plugins from XDG data directory\n\nPlugins placed in ~/.local/share/kasane/plugins/*.wasm are automatically\nloaded at startup. Config options in [plugins] section:\n- auto_discover (default: true)\n- path (custom plugins directory)\n- disabled (list of plugin IDs to skip)\n\nwasm-plugins feature is now enabled by default in the kasane binary.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T16:09:27+09:00",
          "tree_id": "cc46b303f3f96f085e8343ff9ce24707a921af76",
          "url": "https://github.com/Yus314/kasane/commit/13ba03721f2709b9250fa7c32a62e5b9c842c040"
        },
        "date": 1773386494528,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 154123,
            "range": "± 3011",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 829657,
            "range": "± 7128",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2035,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 141113,
            "range": "± 3004",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4821014,
            "range": "± 21946",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16787516,
            "range": "± 76780",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1759516,
            "range": "± 12638",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20068541,
            "range": "± 196168",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7401,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55622,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 598,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7525,
            "range": "± 27",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "dd1c699eb61c54f7e8df5c403e70d5c2308ff237",
          "message": "fix: remove SDK target/ from git and add .gitignore\n\nThe kasane-plugin-sdk is excluded from the workspace, so its target/\ndirectory isn't covered by the root .gitignore.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T16:33:09+09:00",
          "tree_id": "9acd8c2f6ff1df8ee635fc5c1ee94d432c389e2d",
          "url": "https://github.com/Yus314/kasane/commit/dd1c699eb61c54f7e8df5c403e70d5c2308ff237"
        },
        "date": 1773387935118,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148617,
            "range": "± 3951",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 867033,
            "range": "± 58611",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2019,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 136181,
            "range": "± 677",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4840157,
            "range": "± 26102",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16778519,
            "range": "± 95385",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1769503,
            "range": "± 11008",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20188588,
            "range": "± 84675",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6868,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55554,
            "range": "± 1738",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 583,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7644,
            "range": "± 64",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "committer": {
            "email": "shizhaoyoujie@gmail.com",
            "name": "Yus314",
            "username": "Yus314"
          },
          "distinct": true,
          "id": "4e23ada9c41745019c61517a5b091edf928b2a91",
          "message": "docs: update architecture to three-layer model after plugin migration\n\nCollapse the four-layer model (upstream/core/built-in plugins/external plugins)\ninto three layers (upstream/core/plugins) across all documentation. Built-in\nand external plugins are now unified under a single plugin layer with three\ndistribution forms: bundled WASM, FS-discovered WASM, and native.\n\nUpdate all plugin references: CursorLinePlugin → cursor_line,\nColorPreviewPlugin → color_preview (bundled WASM plugins).\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T18:00:45+09:00",
          "tree_id": "cca6fac6cfa0d021facce1d94c009b5897d20c3f",
          "url": "https://github.com/Yus314/kasane/commit/4e23ada9c41745019c61517a5b091edf928b2a91"
        },
        "date": 1773393161831,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 123944,
            "range": "± 4888",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 705036,
            "range": "± 7772",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1750,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 118545,
            "range": "± 1576",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4131575,
            "range": "± 19516",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16949580,
            "range": "± 52294",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1715353,
            "range": "± 9872",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19853777,
            "range": "± 128374",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6914,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 53982,
            "range": "± 1114",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 508,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 6605,
            "range": "± 17",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}