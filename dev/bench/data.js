window.BENCHMARK_DATA = {
  "lastUpdate": 1772938844236,
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
      }
    ]
  }
}