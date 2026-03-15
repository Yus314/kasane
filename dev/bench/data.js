window.BENCHMARK_DATA = {
  "lastUpdate": 1773603333529,
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
          "id": "c3f7822b0cdfd71aaa7a0722a6ce88fea8dd75d8",
          "message": "feat(wasm): upgrade WIT to v0.3.0 — close native Plugin trait feature gap\n\nAdd all missing Plugin trait capabilities to the WASM interface:\n- G1: Decorator system (decorate + decorator-priority exports, element injection)\n- G2: Replacement system (replace-target enum + replace export)\n- G3: Menu transformation (transform-menu-item export, native↔WIT atom conversion)\n- G4: Inter-plugin messaging (update export, Vec<u8> payload downcast)\n- G5: Host state expansion (Tier 1-3: status bar, menu/info, ui-options, faces)\n- G6: Command expansion (schedule-timer, plugin-message variants)\n- G7: Element builder expansion (create-container-styled, create-scrollable, create-stack)\n\nSDK adds default_menu_transform/replace/decorate/decorator_priority/update macros.\nAll three bundled guests updated and rebuilt for v0.3.0.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T18:22:33+09:00",
          "tree_id": "7135b77de4856e4ba6466f39a1ec64f54ab4e6f1",
          "url": "https://github.com/Yus314/kasane/commit/c3f7822b0cdfd71aaa7a0722a6ce88fea8dd75d8"
        },
        "date": 1773394481173,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148686,
            "range": "± 6048",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 811298,
            "range": "± 43033",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2037,
            "range": "± 141",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 140996,
            "range": "± 315",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4764366,
            "range": "± 118602",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16734006,
            "range": "± 71800",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1750333,
            "range": "± 6593",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20010535,
            "range": "± 471127",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6928,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55361,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 722,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7514,
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
          "id": "798aed381dd114974ff219b1be6b90b15f76fcaa",
          "message": "feat(wasm): add sel-badge bundled plugin and rebuild guests for WIT v0.4.0\n\nSelection badge (\" N sel \") is now a bundled WASM plugin contributing to\nSlot::StatusRight. All WASM guests rebuilt against WIT v0.4.0 with\ncursor-style-override and contribute-named default impls.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T19:02:26+09:00",
          "tree_id": "411058a20895090c49c6612974e09531a4184e97",
          "url": "https://github.com/Yus314/kasane/commit/798aed381dd114974ff219b1be6b90b15f76fcaa"
        },
        "date": 1773396891227,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 143210,
            "range": "± 5090",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 861543,
            "range": "± 8147",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2126,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 142473,
            "range": "± 9695",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4823759,
            "range": "± 11935",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16871102,
            "range": "± 199535",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1771941,
            "range": "± 7830",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20261251,
            "range": "± 58802",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6856,
            "range": "± 68",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55199,
            "range": "± 4669",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 617,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7441,
            "range": "± 72",
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
          "id": "2642e904defbc2086c510378dcc2797308819591",
          "message": "feat(plugin): add capability indexing, overlay caching, and PaintPatch integration\n\nPhase 1 of WASM-first architecture evolution:\n\n- Wire render_pipeline_patched() into TUI event loop with StatusBarPatch,\n  MenuSelectionPatch, and CursorPatch for fast-path cell-level updates\n- Add plugin state hash guard: patches are refused when any plugin's\n  state_hash() changed, preventing stale output from skipped view layer\n- Add PluginCapabilities bitflags to skip WASM boundary crossings for\n  non-participating plugins (9 capabilities: SLOT_CONTRIBUTOR,\n  LINE_DECORATION, OVERLAY, DECORATOR, REPLACEMENT, MENU_TRANSFORM,\n  CURSOR_STYLE, INPUT_HANDLER, NAMED_SLOT)\n- Add L3 overlay caching with overlay_deps() dirty flag intersection,\n  matching existing slot caching strategy\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T20:30:32+09:00",
          "tree_id": "a9997288407b04abaa2ac932bbaf08551951c24a",
          "url": "https://github.com/Yus314/kasane/commit/2642e904defbc2086c510378dcc2797308819591"
        },
        "date": 1773402154571,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 125919,
            "range": "± 603",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 724720,
            "range": "± 23788",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1776,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 121180,
            "range": "± 374",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4172734,
            "range": "± 15510",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 17124857,
            "range": "± 116508",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1724469,
            "range": "± 2377",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20146356,
            "range": "± 109031",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7398,
            "range": "± 132",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 51780,
            "range": "± 193",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 533,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 6640,
            "range": "± 42",
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
          "id": "c9ed533f8ec7863457448fb4dc7e828fdfe0eba7",
          "message": "feat(surface): add Surface Model architecture with workspace, ephemeral surfaces, and slot discovery\n\nIntroduce the Surface abstraction that treats all screen regions as\nfirst-class objects, eliminating the core/plugin asymmetry. Core\ncomponents (buffer, status bar, menu, info popups) and plugin-provided\nsurfaces now share the same trait and lifecycle.\n\nKey additions:\n- Surface trait with view/event/slot APIs (surface/mod.rs)\n- SurfaceRegistry with compose_view, route_event, sync_ephemeral_surfaces\n- Workspace generalized pane manager using SurfaceId (workspace.rs)\n- KakouneBufferSurface delegates to view_cached() for pixel-identical output\n- MenuSurface/InfoSurface as ephemeral floating surfaces\n- Surface-local named slots: all_declared_slots() and slot_owner()\n- Plugin capabilities: SURFACE_PROVIDER, WORKSPACE_OBSERVER\n- TUI and GUI event loops wired with SurfaceRegistry + workspace dispatch\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T21:44:36+09:00",
          "tree_id": "15fae3a8f6dbb0fecd25e829d615a46e05803b24",
          "url": "https://github.com/Yus314/kasane/commit/c9ed533f8ec7863457448fb4dc7e828fdfe0eba7"
        },
        "date": 1773406607469,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 149218,
            "range": "± 1474",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 847823,
            "range": "± 11421",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2119,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 136441,
            "range": "± 489",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4739529,
            "range": "± 13696",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16773461,
            "range": "± 62637",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1759786,
            "range": "± 6295",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20044237,
            "range": "± 420816",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7967,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 55687,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 595,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7464,
            "range": "± 36",
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
          "id": "8e7aff29d1e554e2e9f783b1007f654acdc62c85",
          "message": "feat(core): complete extensibility architecture redesign (Phases 1-5)\n\nImplement the core privilege exclusion plan to make the plugin system\nfully symmetric and extensible:\n\n- Phase 1: Open StyleToken from closed enum to struct(CompactString)\n- Phase 2: Extract BuiltinInputPlugin for overridable PageUp/PageDown\n- Phase 3: Slot enum → SlotId migration with custom slot support\n- Phase 3e: Deprecate Slot enum, migrate internals to SlotId API\n- Phase 4: Activate Surface model as the rendering element source\n- Phase 5: Add PaintHook trait for post-paint grid modifications\n\nBoth TUI and GUI backends now use Surface-based cached rendering\npipelines (render_pipeline_surfaces_patched / scene_render_pipeline_\nsurfaces_cached) with full ViewCache, LayoutCache, and PaintPatch\noptimization support.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-13T23:51:59+09:00",
          "tree_id": "96671f5c42480835d0c9b2eb2eaed0ae272f5ab4",
          "url": "https://github.com/Yus314/kasane/commit/8e7aff29d1e554e2e9f783b1007f654acdc62c85"
        },
        "date": 1773414253019,
        "tool": "cargo",
        "benches": [
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 145205,
            "range": "± 3262",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 836267,
            "range": "± 51277",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2038,
            "range": "± 135",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144200,
            "range": "± 3645",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4807004,
            "range": "± 86330",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16724075,
            "range": "± 70005",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1753535,
            "range": "± 7043",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 20077930,
            "range": "± 75424",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7404,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 56896,
            "range": "± 357",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 594,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7369,
            "range": "± 62",
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
          "id": "1a7da1328871917ff8ce46c0ba0bf0b1e80ee83d",
          "message": "docs: add ADR-015 and update performance docs for rendering pipeline optimization\n\n- Add ADR-015 entry to decisions.md (4-stage rendering pipeline optimization)\n- Update performance.md: frame flow (draw_grid), backend benchmarks,\n  bottleneck analysis (container fill/diff allocation resolved),\n  ADR-015 section with benchmark results, 240Hz analysis\n- Update roadmap.md: mark 4 performance items as done\n- Update CLAUDE.md: rendering pipeline diagram and performance numbers\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T02:38:08+09:00",
          "tree_id": "6c0d79cf236bdef424ef4ee4cc23b1bf325c9a6e",
          "url": "https://github.com/Yus314/kasane/commit/1a7da1328871917ff8ce46c0ba0bf0b1e80ee83d"
        },
        "date": 1773424965040,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 578,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 6858,
            "range": "± 507",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 337,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28245,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97134,
            "range": "± 339",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 32998,
            "range": "± 273",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25181,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13534,
            "range": "± 173",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 31433,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12734,
            "range": "± 71",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3314,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20691,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 39,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 139,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 245,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 249,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 750,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1266,
            "range": "± 49",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 46170,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35710,
            "range": "± 365",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 58133,
            "range": "± 983",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 57975,
            "range": "± 411",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 58394,
            "range": "± 317",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 43280,
            "range": "± 185",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 45588,
            "range": "± 364",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 35779,
            "range": "± 305",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 64064,
            "range": "± 756",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 555290,
            "range": "± 14742",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2713952,
            "range": "± 11208",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3045,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 56505,
            "range": "± 916",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12771,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 50156,
            "range": "± 236",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 266192,
            "range": "± 8597",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 927,
            "range": "± 2879",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5882,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46127,
            "range": "± 184",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 201799,
            "range": "± 889",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 363995,
            "range": "± 867",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2970535,
            "range": "± 76860",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5931041,
            "range": "± 186442",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13507,
            "range": "± 502",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81716,
            "range": "± 802",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 162029,
            "range": "± 551",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 4231,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 3392,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 18201,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5349,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 14681,
            "range": "± 75",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26166,
            "range": "± 133",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 40560,
            "range": "± 166",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1271,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 3895,
            "range": "± 130",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 201,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6246,
            "range": "± 46",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3813,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12934,
            "range": "± 1471",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 6971,
            "range": "± 27311",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_hit",
            "value": 504,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_miss",
            "value": 3289,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 154068,
            "range": "± 1004",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 863549,
            "range": "± 3659",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2065,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 142323,
            "range": "± 1480",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 51373,
            "range": "± 176",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 276798,
            "range": "± 1585",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 21138,
            "range": "± 191",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 51381,
            "range": "± 3810",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 141387,
            "range": "± 1922",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 51116,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3994492,
            "range": "± 20409",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15097644,
            "range": "± 82953",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1584376,
            "range": "± 6770",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 17572956,
            "range": "± 69064",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7398,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54752,
            "range": "± 317",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 595,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7363,
            "range": "± 11",
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
          "id": "33b573602ddece77e68319a50d565afaa8922e99",
          "message": "feat(plugin): redesign plugin system with Contribute/Transform/Annotate API\n\nReplace the existing Slot/Decorator/Replacement mechanism with a unified\nContribute/Transform/Annotate system that provides better composability,\nlayout awareness, and z-ordered background layers.\n\nNew APIs:\n- contribute_to() with ContributeContext (layout constraints + priority)\n- transform() middleware chain (unifies decorate + replace)\n- annotate_line_with_ctx() with BackgroundLayer z-ordering\n- contribute_overlay_with_ctx() with OverlayContext (collision avoidance)\n\nKey design decisions:\n- Capability flags prevent double dispatch (C3)\n- Lazy element construction via FnOnce closures (C7)\n- StatusBarPatch bypass when TRANSFORMER plugins exist (C6)\n- WIT breaking change: all WASM plugins updated simultaneously (C1)\n\nOld APIs are deprecated but remain functional via default fallbacks.\nAll 664 tests pass, clippy clean.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T13:28:29+09:00",
          "tree_id": "0f8f9c55a24e43bc324fd33af1e94e548d3bf1ff",
          "url": "https://github.com/Yus314/kasane/commit/33b573602ddece77e68319a50d565afaa8922e99"
        },
        "date": 1773463991126,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 558,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 8271,
            "range": "± 49",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 333,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28291,
            "range": "± 707",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97070,
            "range": "± 496",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 32965,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25159,
            "range": "± 560",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13577,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 32676,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12619,
            "range": "± 86",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3308,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20805,
            "range": "± 184",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 39,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 138,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 248,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 230,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 695,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1217,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 45711,
            "range": "± 214",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35095,
            "range": "± 14598",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 59640,
            "range": "± 425",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 60852,
            "range": "± 540",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 60771,
            "range": "± 334",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 42684,
            "range": "± 320",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 45081,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 35161,
            "range": "± 2023",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65397,
            "range": "± 1441",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 569251,
            "range": "± 15604",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2777559,
            "range": "± 161176",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3023,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 58221,
            "range": "± 582",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 13071,
            "range": "± 2064",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 50094,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 264521,
            "range": "± 2237",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1123,
            "range": "± 3353",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5543,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 45705,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 199618,
            "range": "± 604",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 357712,
            "range": "± 977",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3020718,
            "range": "± 32891",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6024180,
            "range": "± 27683",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12748,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 77644,
            "range": "± 1233",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 154788,
            "range": "± 318",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 7021,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 5789,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 18730,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5221,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 16922,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26223,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 44328,
            "range": "± 331",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1257,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6323,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 210,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6300,
            "range": "± 49",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3747,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12975,
            "range": "± 89",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 6971,
            "range": "± 249",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_hit",
            "value": 509,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_miss",
            "value": 3405,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 149321,
            "range": "± 4462",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 902007,
            "range": "± 10897",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2122,
            "range": "± 142",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144060,
            "range": "± 689",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 52171,
            "range": "± 526",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 281960,
            "range": "± 2541",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 18046,
            "range": "± 61",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 50384,
            "range": "± 1405",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 145295,
            "range": "± 311",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 50381,
            "range": "± 275",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4017993,
            "range": "± 45294",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15140915,
            "range": "± 101592",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1647250,
            "range": "± 3795",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18040669,
            "range": "± 68242",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6836,
            "range": "± 102",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54155,
            "range": "± 273",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 597,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 8173,
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
          "id": "ef4864dce41cbc81b5b1d34b0c9207c3cac92294",
          "message": "feat(render): split DirtyFlags::BUFFER, add stable() macro, LineAnnotation priority, and trace-equivalence tests\n\nTheoretical foundation strengthening across 4 phases:\n\nPhase A — Zero-Cost Foundations:\n- Document 3-phase transform chain semantics (Seed/Default/Chain) on\n  apply_transform_chain(), ReplaceTarget, Plugin::replace()\n- Categorize AppState fields as Observed/Derived/Heuristic with doc annotations\n- Add stable() attribute to #[kasane_component] for intentional staleness\n  policy (vs allow() as escape hatch); migrate build_info_section\n\nPhase B — Test Infrastructure:\n- Add proptest-based trace-equivalence tests verifying all pipeline variants\n  (cached, sectioned, patched) produce identical CellGrid output\n- Covers warm/cold cache transitions, random state mutations, 5 state\n  configurations × 8 flag combinations\n\nPhase C — Optimization:\n- Split DirtyFlags::BUFFER into BUFFER_CONTENT (1<<0) + BUFFER_CURSOR (1<<6)\n  enabling cursor-only mode changes to skip base ViewCache rebuild\n- Remove spurious BUFFER dep from build_status_bar (only reads STATUS fields)\n- Refine flag granularity: DrawStatus mode_change → STATUS|BUFFER_CURSOR,\n  MenuHide → MENU|BUFFER_CONTENT, InfoHide → INFO|BUFFER_CONTENT\n- Add priority: i16 to LineAnnotation with gutter sort ordering\n- Update WASM SDK (dirty::BUFFER_CONTENT, dirty::BUFFER_CURSOR, ALL=0x7F),\n  WIT interface (line-annotation priority field), and recompile all guests\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T15:59:21+09:00",
          "tree_id": "b3e3063b5950df2f061013fb0f406f9d3359d64e",
          "url": "https://github.com/Yus314/kasane/commit/ef4864dce41cbc81b5b1d34b0c9207c3cac92294"
        },
        "date": 1773473060451,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 575,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 7829,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 341,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28340,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97483,
            "range": "± 578",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 33077,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25172,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12741,
            "range": "± 74",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 32740,
            "range": "± 85",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12618,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3311,
            "range": "± 23",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20765,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/1",
            "value": 40,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/5",
            "value": 139,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "decorator_chain/plugins/10",
            "value": 242,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/1",
            "value": 238,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/5",
            "value": 742,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "plugin_dispatch/plugins/10",
            "value": 1188,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 46511,
            "range": "± 883",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35256,
            "range": "± 12592",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 59700,
            "range": "± 1621",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 60153,
            "range": "± 410",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 60819,
            "range": "± 454",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 43797,
            "range": "± 280",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 46136,
            "range": "± 414",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 35925,
            "range": "± 991",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 64646,
            "range": "± 773",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 565828,
            "range": "± 15091",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2730921,
            "range": "± 15915",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3070,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 57501,
            "range": "± 371",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12944,
            "range": "± 1724",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 51347,
            "range": "± 156",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 271286,
            "range": "± 828",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 814,
            "range": "± 1717",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5936,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46578,
            "range": "± 325",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 200626,
            "range": "± 334",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 359694,
            "range": "± 667",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3021149,
            "range": "± 11719",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5992241,
            "range": "± 17007",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12749,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81797,
            "range": "± 141",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 153988,
            "range": "± 446",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 7014,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6152,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 17454,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5011,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17438,
            "range": "± 115",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26388,
            "range": "± 171",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 44656,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1285,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6412,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 204,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6240,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3794,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 13136,
            "range": "± 68",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 6604,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_hit",
            "value": 511,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "collect_slot_cached_miss",
            "value": 3482,
            "range": "± 59",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 144677,
            "range": "± 412",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 832794,
            "range": "± 4827",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2064,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 143009,
            "range": "± 2966",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 50906,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 279034,
            "range": "± 848",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 18032,
            "range": "± 256",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 49941,
            "range": "± 174",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 136980,
            "range": "± 1917",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 49869,
            "range": "± 184",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4054632,
            "range": "± 21545",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15471001,
            "range": "± 63395",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1666207,
            "range": "± 8827",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18263028,
            "range": "± 47828",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6887,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54780,
            "range": "± 164",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 653,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7449,
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
          "id": "90da1bb267facf87ee10092ea306b7faa10efcd1",
          "message": "refactor: remove deprecated plugin APIs, split plugin.rs, unify render pipeline\n\nPhase 1: Remove deprecated Slot/Decorator/Replacement/LineDecoration APIs\n- Delete Slot enum, DecorateTarget, ReplaceTarget, LineDecoration types\n- Remove deprecated Plugin trait methods (contribute, decorate, replace,\n  contribute_line, slot_deps, contribute_slot, slot_id_deps)\n- Remove deprecated PluginCapabilities flags (SLOT_CONTRIBUTOR, DECORATOR,\n  REPLACEMENT, LINE_DECORATION, NAMED_SLOT)\n- Remove deprecated PluginRegistry methods (collect_slot, build_left_gutter,\n  build_right_gutter, collect_line_backgrounds, apply_decorator, get_replacement)\n- Clean up proc macro codegen, SDK macros, WASM adapter, tests, benchmarks\n- Migrate examples/line-numbers to new contribute_to API\n\nPhase 2: Split 3,302-line plugin.rs into plugin/ module directory\n- mod.rs (PluginCapabilities, PluginId, SlotId, re-exports)\n- traits.rs (Plugin trait)\n- context.rs (ContributeContext, TransformContext, AnnotateContext, etc.)\n- command.rs (PaintHook, Command, DeferredCommand, extract/execute)\n- registry.rs (PluginRegistry, PluginSlotCache, EffectiveSectionDeps)\n- tests.rs (unit tests)\n\nPhase 3: Unify render pipeline via ViewSource trait\n- Create ViewSource trait abstracting PluginRegistry vs SurfaceRegistry\n- Replace 4 pairs of duplicated pipeline functions with generic core functions\n- Extract build_sections_with_base helper in view/mod.rs\n\nPhase 4: Extract dispatch_workspace_command to kasane-core\n- Move identical function from TUI/GUI backends to workspace.rs\n\nPhase 5: Add bidirectional_enum! macro for WASM type conversions\n- Replace NamedColor WIT↔native conversion boilerplate (32 lines → 4)\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T17:06:07+09:00",
          "tree_id": "b63667f88d6ba7826222a6904a1438b73d4a70be",
          "url": "https://github.com/Yus314/kasane/commit/90da1bb267facf87ee10092ea306b7faa10efcd1"
        },
        "date": 1773476928581,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 520,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 4207,
            "range": "± 44",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 319,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 25463,
            "range": "± 79",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 86320,
            "range": "± 219",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 29531,
            "range": "± 623",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 20568,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 10471,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 25285,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 10456,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 2946,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 18308,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 40451,
            "range": "± 266",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 31755,
            "range": "± 7176",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 53884,
            "range": "± 547",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 54262,
            "range": "± 364",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 54787,
            "range": "± 316",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 37951,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 39507,
            "range": "± 252",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 31796,
            "range": "± 733",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65665,
            "range": "± 499",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 590132,
            "range": "± 10924",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 3141913,
            "range": "± 42659",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3224,
            "range": "± 100",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 59835,
            "range": "± 563",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12316,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 48484,
            "range": "± 136",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 259088,
            "range": "± 551",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1077,
            "range": "± 1325",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5561,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 40552,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 173485,
            "range": "± 389",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 321599,
            "range": "± 3032",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3363361,
            "range": "± 70908",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6739190,
            "range": "± 40398",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 10481,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 63672,
            "range": "± 336",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 127362,
            "range": "± 1106",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 6956,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 5858,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 18820,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6553,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 18010,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 24072,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 39990,
            "range": "± 264",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1178,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6329,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 197,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 5534,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3487,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12211,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 8442,
            "range": "± 37709",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 127585,
            "range": "± 393",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 732435,
            "range": "± 2994",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1805,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 122417,
            "range": "± 199",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 44605,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 239961,
            "range": "± 717",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 18768,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 43703,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 122006,
            "range": "± 211",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 43680,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3466114,
            "range": "± 8766",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15690637,
            "range": "± 66207",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1613895,
            "range": "± 3568",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18216849,
            "range": "± 65567",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6972,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 52471,
            "range": "± 696",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 492,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 6132,
            "range": "± 67",
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
          "id": "c356f2653182abb4d4d493842c51b59eaeff2057",
          "message": "refactor: deduplicate event loop, layout split, and test helpers across backends\n\nExtract deferred command handling into kasane-core/src/event_loop.rs with\nTimerScheduler trait, eliminating ~60 lines of duplicated dispatch logic\nbetween TUI and GUI backends. Consolidate SplitDirection and split_rect()\ninto Rect::split() in layout/mod.rs (shared by pane and workspace).\nMove row_text, render_with_registry, render_to_grid, assert_grids_equal\ninto test_support.rs, deduplicating helpers across 4 integration test files.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T17:33:47+09:00",
          "tree_id": "03fbb2fee4d83c1ff4d67fede9975aa195d4e726",
          "url": "https://github.com/Yus314/kasane/commit/c356f2653182abb4d4d493842c51b59eaeff2057"
        },
        "date": 1773478627590,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 562,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 4506,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 336,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28519,
            "range": "± 1284",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97939,
            "range": "± 786",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 33258,
            "range": "± 170",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25160,
            "range": "± 296",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13518,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 31287,
            "range": "± 77",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12739,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3317,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20928,
            "range": "± 93",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 46417,
            "range": "± 300",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35669,
            "range": "± 196",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 60729,
            "range": "± 314",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 61491,
            "range": "± 402",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 61850,
            "range": "± 357",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 43527,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 45562,
            "range": "± 207",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 35803,
            "range": "± 1078",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65481,
            "range": "± 623",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 571199,
            "range": "± 16049",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2789559,
            "range": "± 34527",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3076,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 56129,
            "range": "± 1076",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12685,
            "range": "± 1844",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 49661,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 262254,
            "range": "± 805",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 967,
            "range": "± 985",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5490,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46449,
            "range": "± 145",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 202913,
            "range": "± 728",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 365051,
            "range": "± 698",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3046798,
            "range": "± 10873",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6112595,
            "range": "± 31587",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13328,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81264,
            "range": "± 238",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 161678,
            "range": "± 545",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 6953,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 5982,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 17737,
            "range": "± 97",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5089,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 16708,
            "range": "± 183",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26583,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 44398,
            "range": "± 575",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1295,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6305,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 220,
            "range": "± 0",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6255,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3770,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12923,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 6952,
            "range": "± 50283",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 149957,
            "range": "± 1021",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 879051,
            "range": "± 10540",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2112,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 140655,
            "range": "± 1630",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 52787,
            "range": "± 171",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 287927,
            "range": "± 699",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 20228,
            "range": "± 160",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 51425,
            "range": "± 258",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 150595,
            "range": "± 3315",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 51739,
            "range": "± 450",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4038260,
            "range": "± 14203",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15339174,
            "range": "± 95297",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1655605,
            "range": "± 6227",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18204994,
            "range": "± 53928",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7414,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54928,
            "range": "± 314",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 594,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7361,
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
          "id": "c58146008760fc8d0aa72deb2a427cbd2f148847",
          "message": "docs: align plugin API docs with current unified API\n\nPlugin trait underwent a major API migration (contribute_to/transform/\nannotate_line_with_ctx/contribute_overlay_with_ctx) but documentation\nstill referenced the old APIs (contribute/decorate/replace/\ncontribute_line/contribute_overlay). Update all docs to reflect the\ncurrent canonical API:\n\n- semantics.md: rewrite §8 extension points and composition order,\n  mark §12.6 (transform/replacement unification) as resolved\n- layer-responsibilities.md: update API verification table and examples\n- plugin-api.md: rewrite mechanism guide, code examples, and\n  PluginCapabilities table with actual flag names\n- plugin-development.md: update quick start examples (WASM + native),\n  fix test example to use collect_contributions, update reference table\n- decisions.md: correct ADR-012 §12-3 WIT parity statement\n- kasane-plugin-sdk/src/lib.rs: update doc comment quick start\n- CLAUDE.md: update plugin system description\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-14T21:45:18+09:00",
          "tree_id": "6e75f4cbff3ab8046b4b142b75e1a6ebe15547a8",
          "url": "https://github.com/Yus314/kasane/commit/c58146008760fc8d0aa72deb2a427cbd2f148847"
        },
        "date": 1773493717911,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 566,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 4578,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 335,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28120,
            "range": "± 327",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97115,
            "range": "± 396",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 32969,
            "range": "± 382",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25180,
            "range": "± 379",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13550,
            "range": "± 578",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 31542,
            "range": "± 1342",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12741,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3309,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20692,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 46064,
            "range": "± 499",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35638,
            "range": "± 13916",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 60246,
            "range": "± 470",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 60850,
            "range": "± 554",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 61109,
            "range": "± 454",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 43178,
            "range": "± 209",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 45050,
            "range": "± 262",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 35666,
            "range": "± 1170",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 64737,
            "range": "± 907",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 564737,
            "range": "± 14586",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2749334,
            "range": "± 34585",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3065,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 55309,
            "range": "± 1349",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12692,
            "range": "± 1865",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 49677,
            "range": "± 193",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 262169,
            "range": "± 810",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 869,
            "range": "± 482",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5526,
            "range": "± 197",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46131,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 203317,
            "range": "± 827",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 364716,
            "range": "± 966",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3036206,
            "range": "± 21568",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6090459,
            "range": "± 48242",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13524,
            "range": "± 442",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 80904,
            "range": "± 259",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 161704,
            "range": "± 2631",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 7042,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6183,
            "range": "± 157",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 17869,
            "range": "± 370",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5114,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17521,
            "range": "± 95",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26156,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 44240,
            "range": "± 115",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1274,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6518,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 220,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6213,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3748,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12944,
            "range": "± 137",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 7657,
            "range": "± 1702",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 153844,
            "range": "± 2984",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 859175,
            "range": "± 23905",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2121,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 142272,
            "range": "± 3113",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 52561,
            "range": "± 448",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 287455,
            "range": "± 2642",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 20198,
            "range": "± 387",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 51639,
            "range": "± 248",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 138839,
            "range": "± 8375",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 51391,
            "range": "± 438",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4011018,
            "range": "± 9720",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15275187,
            "range": "± 78650",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1651083,
            "range": "± 5505",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18053380,
            "range": "± 82390",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7408,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54938,
            "range": "± 118",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 595,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7481,
            "range": "± 36",
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
          "id": "f032e29ec8631e69a098d30dba7ea0351776b2f2",
          "message": "feat(wasm): implement Phase P-1 WASI capability infrastructure\n\nAdd per-plugin WASI capability declaration and grant system. Plugins\ndeclare needed capabilities (filesystem, environment, monotonic-clock)\nvia WIT export `requested-capabilities`, and the host builds a\nper-plugin WasiCtx based on declarations and user config denials.\n\n- WIT v0.5.0: add `capability` enum and `requested-capabilities` export\n- SDK: add `capability` constants module and `default_capabilities!()` macro\n- New `kasane-wasm/src/capability.rs`: `WasiCapabilityConfig` + `build_wasi_ctx()`\n- Filesystem grants: plugin data dir (read/write) + CWD (read-only)\n- Config: `[plugins.deny_capabilities]` for per-plugin capability denial\n- Update `WasmPluginLoader::load()` to query and apply capabilities\n- Rebuild all bundled/fixture .wasm binaries with new WIT interface\n- 7 new tests (5 capability, 2 config)\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T11:56:48+09:00",
          "tree_id": "45f30677202c47cd72a8dd4699de5a8811c10c69",
          "url": "https://github.com/Yus314/kasane/commit/f032e29ec8631e69a098d30dba7ea0351776b2f2"
        },
        "date": 1773544800111,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 567,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 4523,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 333,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 28453,
            "range": "± 291",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 97789,
            "range": "± 1488",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 33143,
            "range": "± 219",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 25193,
            "range": "± 353",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12758,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 32665,
            "range": "± 132",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12624,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3314,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20734,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 45638,
            "range": "± 180",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 35002,
            "range": "± 14013",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 59723,
            "range": "± 444",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 59885,
            "range": "± 459",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 60566,
            "range": "± 1001",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 42825,
            "range": "± 308",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 44989,
            "range": "± 685",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 34985,
            "range": "± 980",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65083,
            "range": "± 1180",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 571179,
            "range": "± 15640",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2810180,
            "range": "± 36619",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3149,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 55432,
            "range": "± 2674",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12738,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 49878,
            "range": "± 241",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 263078,
            "range": "± 1708",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1284,
            "range": "± 3167",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5586,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 46470,
            "range": "± 196",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 199534,
            "range": "± 643",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 358272,
            "range": "± 1667",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3049691,
            "range": "± 53620",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6150159,
            "range": "± 119316",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13554,
            "range": "± 270",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 81642,
            "range": "± 1211",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 154402,
            "range": "± 335",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 7073,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6125,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 17649,
            "range": "± 211",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 5395,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17221,
            "range": "± 74",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 26274,
            "range": "± 68",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 43729,
            "range": "± 167",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1281,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6575,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 220,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 6253,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3813,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12883,
            "range": "± 1559",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 7377,
            "range": "± 32754",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 151970,
            "range": "± 2050",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 843030,
            "range": "± 17799",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2003,
            "range": "± 23",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 143789,
            "range": "± 952",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 52444,
            "range": "± 182",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 283787,
            "range": "± 1149",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 20347,
            "range": "± 106",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 50728,
            "range": "± 158",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 140211,
            "range": "± 4957",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 50761,
            "range": "± 179",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4041628,
            "range": "± 18726",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15250069,
            "range": "± 50770",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1661376,
            "range": "± 17174",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18118415,
            "range": "± 61713",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 7406,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 54782,
            "range": "± 1261",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 572,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 7577,
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
          "id": "ad1148b3debffc4a1555a9766472d0bf1cb4b58c",
          "message": "feat(wasm): implement Phase P-2 process execution infrastructure\n\nAdd host-mediated process execution for WASM plugins following ADR-019's\nhybrid I/O model. Plugins spawn processes via Command::SpawnProcess and\nreceive stdout/stderr/exit events through Plugin::on_io_event().\n\nCore types (kasane-core):\n- IoEvent/ProcessEvent/StdinMode types in new plugin/io.rs\n- Command::SpawnProcess/WriteToProcess/CloseProcessStdin/KillProcess\n- ProcessDispatcher/ProcessEventSink traits for backend abstraction\n- Plugin::on_io_event() + allows_process_spawn() trait methods\n- PluginRegistry::deliver_io_event() + capability check in event_loop\n\nWASM adapter (kasane-wasm):\n- WIT v0.6.0: io-event, process-event, on-io-event, capability::process\n- Conversion functions and WasmPlugin.on_io_event() adapter\n- is_capability_granted() for process capability resolution\n- Rebuilt all bundled/fixture WASM plugins\n\nProcess manager (kasane/):\n- Tokio-based ProcessManager with per-plugin limits (4/plugin, 16 total)\n- Async stdout/stderr/stdin management via tokio::select!\n\nEvent loop integration:\n- TUI: Event::ProcessOutput + TuiProcessEventSink + factory pattern\n- GUI: GuiEvent::ProcessOutput + GuiProcessEventSink + factory pattern\n- Tokio runtime created in kasane/src/lib.rs, passed via factory closure\n\nTests: 34 new tests across kasane-core, kasane-wasm, and kasane/ crates.\nDocs: Updated roadmap, plugin-api, plugin-development, and ADR-019.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T13:13:50+09:00",
          "tree_id": "56bcc84754bec79e8eb995aadd0319dd75fb281c",
          "url": "https://github.com/Yus314/kasane/commit/ad1148b3debffc4a1555a9766472d0bf1cb4b58c"
        },
        "date": 1773549402222,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 526,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 4190,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 313,
            "range": "± 8",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 25063,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 86186,
            "range": "± 361",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 29337,
            "range": "± 515",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 21687,
            "range": "± 37",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 10606,
            "range": "± 138",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 25354,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 11233,
            "range": "± 99",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 2942,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 18366,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 40345,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 31605,
            "range": "± 4761",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 54052,
            "range": "± 461",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 54218,
            "range": "± 343",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 54327,
            "range": "± 306",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 37560,
            "range": "± 190",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 39207,
            "range": "± 158",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 31866,
            "range": "± 724",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 65666,
            "range": "± 876",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 591334,
            "range": "± 10801",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 3128466,
            "range": "± 34329",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3110,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 60192,
            "range": "± 503",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 12786,
            "range": "± 1020",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 50476,
            "range": "± 303",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 270310,
            "range": "± 866",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1133,
            "range": "± 762",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5986,
            "range": "± 159",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 40396,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 173339,
            "range": "± 525",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 319718,
            "range": "± 657",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3356706,
            "range": "± 17845",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6842422,
            "range": "± 45875",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 10621,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 63980,
            "range": "± 680",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 128729,
            "range": "± 3409",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 7024,
            "range": "± 24",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 5896,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 19025,
            "range": "± 101",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6552,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 18406,
            "range": "± 51",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 23549,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 39563,
            "range": "± 58",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 1187,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6326,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 193,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 5512,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 3447,
            "range": "± 7",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 12601,
            "range": "± 199",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 8371,
            "range": "± 888",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 127124,
            "range": "± 870",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 722572,
            "range": "± 3572",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1798,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 121465,
            "range": "± 437",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 44852,
            "range": "± 111",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 242883,
            "range": "± 917",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 18763,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 44626,
            "range": "± 122",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 121313,
            "range": "± 434",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 44636,
            "range": "± 88",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3470729,
            "range": "± 20931",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 15689651,
            "range": "± 36794",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1614612,
            "range": "± 2945",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 18181203,
            "range": "± 67351",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 6961,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 53400,
            "range": "± 1519",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 511,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 6732,
            "range": "± 95",
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
          "id": "32ff7db0d612e9d55fd92ad5c1daf6db64108b82",
          "message": "Implement hosted surfaces and session runtime groundwork",
          "timestamp": "2026-03-15T20:18:19+09:00",
          "tree_id": "c1326cac8aed3d335fb038def03b86b90bf4e525",
          "url": "https://github.com/Yus314/kasane/commit/32ff7db0d612e9d55fd92ad5c1daf6db64108b82"
        },
        "date": 1773574872877,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 5306,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 10452,
            "range": "± 40",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1109,
            "range": "± 25",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 30311,
            "range": "± 134",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 102663,
            "range": "± 382",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 35671,
            "range": "± 229",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 39704,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 14049,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27754,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12732,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3289,
            "range": "± 9",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20582,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 55058,
            "range": "± 178",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45289,
            "range": "± 4898",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 69761,
            "range": "± 632",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 70342,
            "range": "± 738",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 70882,
            "range": "± 583",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 51908,
            "range": "± 475",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 53905,
            "range": "± 933",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 44763,
            "range": "± 1149",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 61796,
            "range": "± 836",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 538546,
            "range": "± 14699",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2642318,
            "range": "± 13306",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2988,
            "range": "± 34",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 54097,
            "range": "± 829",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 15719,
            "range": "± 1943",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 63542,
            "range": "± 150",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 336553,
            "range": "± 1035",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 884,
            "range": "± 1496",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5150,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 54945,
            "range": "± 166",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 220020,
            "range": "± 670",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 390584,
            "range": "± 1462",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2961587,
            "range": "± 13796",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5908016,
            "range": "± 27162",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 14051,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 85830,
            "range": "± 237",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 169855,
            "range": "± 517",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11110,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6378,
            "range": "± 48",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 23955,
            "range": "± 285",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6666,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17648,
            "range": "± 183",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 36626,
            "range": "± 78",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 50130,
            "range": "± 122",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 6112,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 7051,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 962,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 13988,
            "range": "± 72",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11543,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 15705,
            "range": "± 85",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 15192,
            "range": "± 8387",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150470,
            "range": "± 644",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 866534,
            "range": "± 2807",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2211,
            "range": "± 17",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144364,
            "range": "± 251",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 48230,
            "range": "± 316",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 259395,
            "range": "± 1535",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 15347,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 46889,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 150569,
            "range": "± 2591",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 46521,
            "range": "± 266",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4416444,
            "range": "± 15589",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16105790,
            "range": "± 148569",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1774433,
            "range": "± 3264",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19390276,
            "range": "± 49896",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 3972,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 50454,
            "range": "± 189",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 469,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2234,
            "range": "± 11",
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
          "id": "434eaaa559b507e3f3fc0bc5ea8d265f3ee86f74",
          "message": "ci: add GitHub Releases workflow for prebuilt binaries\n\nBuild and publish prebuilt binaries for 5 targets on v* tags:\nx86_64/aarch64 Linux (GNU + musl), x86_64/aarch64 macOS.\nIncludes SHA256 checksums and auto-generated release notes.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T20:26:02+09:00",
          "tree_id": "c1326cac8aed3d335fb038def03b86b90bf4e525",
          "url": "https://github.com/Yus314/kasane/commit/434eaaa559b507e3f3fc0bc5ea8d265f3ee86f74"
        },
        "date": 1773575478093,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 5299,
            "range": "± 28",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 10497,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1114,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 30232,
            "range": "± 107",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 103463,
            "range": "± 497",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 35546,
            "range": "± 131",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 39757,
            "range": "± 233",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 14054,
            "range": "± 109",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27731,
            "range": "± 545",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12903,
            "range": "± 52",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3295,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20805,
            "range": "± 156",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 55322,
            "range": "± 256",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45847,
            "range": "± 9642",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 70054,
            "range": "± 815",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 70394,
            "range": "± 607",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 71163,
            "range": "± 735",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 52227,
            "range": "± 551",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 53899,
            "range": "± 649",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 45157,
            "range": "± 817",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 62004,
            "range": "± 1805",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 540332,
            "range": "± 15216",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2647638,
            "range": "± 35764",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2896,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 54760,
            "range": "± 401",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 16109,
            "range": "± 1699",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 64186,
            "range": "± 352",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 337910,
            "range": "± 1061",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1202,
            "range": "± 2848",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5473,
            "range": "± 208",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 55239,
            "range": "± 679",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 220561,
            "range": "± 799",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 390642,
            "range": "± 1974",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2967683,
            "range": "± 24459",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 5924339,
            "range": "± 41469",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 14643,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 89419,
            "range": "± 266",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 169795,
            "range": "± 582",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11770,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6323,
            "range": "± 120",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 24300,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6539,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17942,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 36542,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 50113,
            "range": "± 1657",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 6091,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6837,
            "range": "± 54",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 917,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 13974,
            "range": "± 30",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11552,
            "range": "± 116",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 15752,
            "range": "± 305",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 16435,
            "range": "± 29891",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 143361,
            "range": "± 2404",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 892345,
            "range": "± 22672",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2126,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 139275,
            "range": "± 5450",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 48040,
            "range": "± 322",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 258836,
            "range": "± 4545",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 15409,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 46728,
            "range": "± 123",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 142806,
            "range": "± 4949",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 46956,
            "range": "± 535",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4434723,
            "range": "± 45124",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16190433,
            "range": "± 171733",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1779544,
            "range": "± 5989",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19483355,
            "range": "± 208102",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 4143,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 50466,
            "range": "± 302",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 468,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2231,
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
          "id": "b5286cc92c2f35ea44a7a1e778136ebdc7f8e5e5",
          "message": "fix(ci): use macos-14 for x86_64-apple-darwin (macos-13 deprecated)\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T20:36:26+09:00",
          "tree_id": "ea4038b7e8a081bffde6728d0c50f6b6dc6a7b0a",
          "url": "https://github.com/Yus314/kasane/commit/b5286cc92c2f35ea44a7a1e778136ebdc7f8e5e5"
        },
        "date": 1773575945068,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 5352,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 10278,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1141,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 30194,
            "range": "± 103",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 103225,
            "range": "± 1849",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 35586,
            "range": "± 281",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 39928,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 14062,
            "range": "± 425",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27956,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 12819,
            "range": "± 277",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3287,
            "range": "± 16",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20552,
            "range": "± 322",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 55310,
            "range": "± 278",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45780,
            "range": "± 5406",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 69947,
            "range": "± 955",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 70426,
            "range": "± 1037",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 70793,
            "range": "± 547",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 52085,
            "range": "± 217",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 54279,
            "range": "± 473",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 45088,
            "range": "± 900",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 62232,
            "range": "± 1398",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 539875,
            "range": "± 21625",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2637608,
            "range": "± 11409",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2897,
            "range": "± 27",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 53281,
            "range": "± 925",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 15736,
            "range": "± 1517",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 63736,
            "range": "± 383",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 336906,
            "range": "± 1084",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 1100,
            "range": "± 2875",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5381,
            "range": "± 226",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 55187,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 220218,
            "range": "± 1156",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 389900,
            "range": "± 1753",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 2986905,
            "range": "± 7981",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6049333,
            "range": "± 49905",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 14091,
            "range": "± 85",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 85529,
            "range": "± 281",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 169855,
            "range": "± 5813",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11640,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6208,
            "range": "± 397",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 23873,
            "range": "± 205",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6535,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17597,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 36547,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 50290,
            "range": "± 448",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 6160,
            "range": "± 19",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6682,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 921,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 14044,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11631,
            "range": "± 149",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 15699,
            "range": "± 90",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 16267,
            "range": "± 28722",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 148545,
            "range": "± 2949",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 925545,
            "range": "± 10202",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2160,
            "range": "± 18",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 144498,
            "range": "± 3576",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 48371,
            "range": "± 177",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 257046,
            "range": "± 1209",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 15365,
            "range": "± 523",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 46897,
            "range": "± 117",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 146113,
            "range": "± 1687",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 46802,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4433683,
            "range": "± 9415",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16093311,
            "range": "± 32319",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1764863,
            "range": "± 3991",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19381219,
            "range": "± 676123",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 4216,
            "range": "± 96",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 50436,
            "range": "± 430",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 474,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2236,
            "range": "± 36",
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
          "id": "008c80959f0aa08ee04cf68b2582eb471c7b9390",
          "message": "refactor: reduce duplication and improve code organization across workspace\n\n- Extract SGR helpers (convert_color, convert_attribute, emit_sgr_diff) into kasane-tui/src/sgr.rs, eliminating ~90-line duplication between backend.rs and benches/backend.rs\n- Add ProcessManager helper methods (send_spawn_failed, decrement_plugin_count) to deduplicate error handling and count management\n- Add HostState::store_element() helper in kasane-wasm, replacing 16 occurrences of the 3-line element registration pattern\n- Add rect_to_wit() conversion helper in kasane-wasm, symmetric to existing wit_rect_to_rect()\n- Restrict pipeline.rs API visibility: 4 internal functions to pub(crate), replace wildcard re-export with explicit list\n- Add WorkspaceNode child traversal helpers (for_each_child, for_each_child_mut, any_child) to simplify recursive methods\n- Extract GPU ScreenUniforms and InstanceBuffer into pipeline_common.rs, deduplicating bg_pipeline and border_pipeline\n- Split kasane-tui lib.rs (743 lines) into lib.rs + event_handler.rs for better separation of concerns\n- Introduce DeferredContext struct to replace 11-argument handle_deferred_commands signature\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T20:49:52+09:00",
          "tree_id": "508de00cb1b93e75034a49b2117a14690a589244",
          "url": "https://github.com/Yus314/kasane/commit/008c80959f0aa08ee04cf68b2582eb471c7b9390"
        },
        "date": 1773576730341,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 5437,
            "range": "± 146",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 10609,
            "range": "± 290",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1147,
            "range": "± 43",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 30113,
            "range": "± 214",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 101876,
            "range": "± 454",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 35090,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 39754,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 12904,
            "range": "± 341",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27735,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 13527,
            "range": "± 219",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3291,
            "range": "± 108",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20675,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 53090,
            "range": "± 383",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 43750,
            "range": "± 8681",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 68038,
            "range": "± 679",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 68489,
            "range": "± 601",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 68924,
            "range": "± 620",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 50120,
            "range": "± 347",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 51821,
            "range": "± 470",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 42904,
            "range": "± 16889",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 63877,
            "range": "± 1215",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 557549,
            "range": "± 14486",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2714255,
            "range": "± 155595",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2925,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 55175,
            "range": "± 798",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 15547,
            "range": "± 2012",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 62981,
            "range": "± 1019",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 333883,
            "range": "± 1041",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 787,
            "range": "± 1383",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5142,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 53468,
            "range": "± 318",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 209438,
            "range": "± 3495",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 367344,
            "range": "± 1110",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3029147,
            "range": "± 5640",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6072491,
            "range": "± 28208",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 12901,
            "range": "± 331",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 74217,
            "range": "± 332",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 147820,
            "range": "± 636",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11779,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6330,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 24243,
            "range": "± 174",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6744,
            "range": "± 259",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 18166,
            "range": "± 82",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 36243,
            "range": "± 147",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 50108,
            "range": "± 98",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 6340,
            "range": "± 15",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 7065,
            "range": "± 22",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 978,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 14320,
            "range": "± 81",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11947,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 15550,
            "range": "± 63",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 16073,
            "range": "± 29647",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 150916,
            "range": "± 2497",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 845624,
            "range": "± 17782",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2101,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 140335,
            "range": "± 2192",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 48371,
            "range": "± 200",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 263783,
            "range": "± 901",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 17046,
            "range": "± 66",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 46911,
            "range": "± 169",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 146257,
            "range": "± 1652",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 49096,
            "range": "± 1339",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4417534,
            "range": "± 13547",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16152586,
            "range": "± 69134",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1769295,
            "range": "± 55857",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19269783,
            "range": "± 50024",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 4367,
            "range": "± 67",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 49483,
            "range": "± 159",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 462,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2240,
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
          "id": "c262a755bbf6c27b31b9a18b3d91c09249bc60c2",
          "message": "docs: add hero demo GIF and rewrite README\n\n- Add GPU backend demo GIF (fuzzy finder + cursor line highlight)\n- Rewrite README: clearer structure, Quick Start section, alias suggestion\n- Add Nix installation option\n- Restructure documentation links into \"Going Further\" section\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-15T22:11:04+09:00",
          "tree_id": "3982813d3aa4a4a8bf5fbdac90616d8255f97099",
          "url": "https://github.com/Yus314/kasane/commit/c262a755bbf6c27b31b9a18b3d91c09249bc60c2"
        },
        "date": 1773581603277,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 5383,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 10634,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1104,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 30351,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 102835,
            "range": "± 264",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 35655,
            "range": "± 84",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 40047,
            "range": "± 94",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 13899,
            "range": "± 83",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27963,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 13911,
            "range": "± 104",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 3267,
            "range": "± 10",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 20581,
            "range": "± 112",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 54938,
            "range": "± 1849",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 45334,
            "range": "± 4986",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 69826,
            "range": "± 520",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 70217,
            "range": "± 1314",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 70567,
            "range": "± 643",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 52363,
            "range": "± 1523",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 53861,
            "range": "± 362",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 45192,
            "range": "± 467",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 62775,
            "range": "± 746",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 553368,
            "range": "± 14399",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 2694465,
            "range": "± 7103",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 2978,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 56479,
            "range": "± 512",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 15639,
            "range": "± 55",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 63591,
            "range": "± 314",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 336522,
            "range": "± 1434",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 740,
            "range": "± 1493",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5372,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 54987,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 219964,
            "range": "± 632",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 389099,
            "range": "± 1083",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3030740,
            "range": "± 6317",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6045114,
            "range": "± 24406",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 13910,
            "range": "± 41",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 84973,
            "range": "± 487",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 169263,
            "range": "± 765",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11574,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6366,
            "range": "± 60",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 23448,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 6479,
            "range": "± 31",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 17512,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 36567,
            "range": "± 284",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 50059,
            "range": "± 203",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 6180,
            "range": "± 20",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6985,
            "range": "± 76",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 932,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 14484,
            "range": "± 121",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11932,
            "range": "± 57",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 15682,
            "range": "± 162",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 15099,
            "range": "± 323",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 147892,
            "range": "± 1419",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 877329,
            "range": "± 15206",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 2074,
            "range": "± 42",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 137577,
            "range": "± 1560",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 47632,
            "range": "± 605",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 254774,
            "range": "± 4833",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 15896,
            "range": "± 234",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 46104,
            "range": "± 427",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 140648,
            "range": "± 1130",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 46049,
            "range": "± 478",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 4423779,
            "range": "± 9249",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16200089,
            "range": "± 44573",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1774916,
            "range": "± 8060",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19375169,
            "range": "± 48239",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 4413,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 49441,
            "range": "± 198",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 462,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2244,
            "range": "± 17",
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
          "id": "05de79d2a56c62382312c4a6b04f4b77fcdb168a",
          "message": "fix(core): make widget_columns optional in draw protocol parsing\n\nKakoune versions between 2024.12.09 and HEAD may not include the\nwidget_columns parameter (added in PR #5455, merged 2026-03-11).\nDefault to 0 when the field is absent so Kasane works with any\nKakoune >= 2024.12.09.\n\nCo-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>",
          "timestamp": "2026-03-16T04:08:38+09:00",
          "tree_id": "9d466ebccb6e3f742fa7a8e13a164b547a6ec335",
          "url": "https://github.com/Yus314/kasane/commit/05de79d2a56c62382312c4a6b04f4b77fcdb168a"
        },
        "date": 1773603332649,
        "tool": "cargo",
        "benches": [
          {
            "name": "element_construct/plugins_0",
            "value": 4940,
            "range": "± 87",
            "unit": "ns/iter"
          },
          {
            "name": "element_construct/plugins_10",
            "value": 9972,
            "range": "± 32",
            "unit": "ns/iter"
          },
          {
            "name": "flex_layout",
            "value": 1252,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24",
            "value": 26573,
            "range": "± 45",
            "unit": "ns/iter"
          },
          {
            "name": "paint/200x60",
            "value": 89171,
            "range": "± 352",
            "unit": "ns/iter"
          },
          {
            "name": "paint/80x24_realistic",
            "value": 30807,
            "range": "± 56",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/full_redraw",
            "value": 38023,
            "range": "± 50",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff/incremental",
            "value": 10686,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/full_redraw",
            "value": 27418,
            "range": "± 38",
            "unit": "ns/iter"
          },
          {
            "name": "grid_diff_into/incremental",
            "value": 10766,
            "range": "± 181",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/80x24",
            "value": 2938,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "grid_clear/200x60",
            "value": 18282,
            "range": "± 36",
            "unit": "ns/iter"
          },
          {
            "name": "full_frame",
            "value": 48158,
            "range": "± 223",
            "unit": "ns/iter"
          },
          {
            "name": "draw_message",
            "value": 39327,
            "range": "± 4102",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/10",
            "value": 60394,
            "range": "± 615",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/50",
            "value": 60813,
            "range": "± 1384",
            "unit": "ns/iter"
          },
          {
            "name": "menu_show/items/100",
            "value": 61250,
            "range": "± 861",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/1",
            "value": 45305,
            "range": "± 153",
            "unit": "ns/iter"
          },
          {
            "name": "incremental_edit/lines/5",
            "value": 46871,
            "range": "± 129",
            "unit": "ns/iter"
          },
          {
            "name": "message_sequence",
            "value": 39382,
            "range": "± 754",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/10",
            "value": 64303,
            "range": "± 518",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/100",
            "value": 577861,
            "range": "± 10549",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_lines/500",
            "value": 3078337,
            "range": "± 20126",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/draw_status",
            "value": 3068,
            "range": "± 33",
            "unit": "ns/iter"
          },
          {
            "name": "parse_request/menu_show_50",
            "value": 58982,
            "range": "± 479",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/23",
            "value": 15659,
            "range": "± 341",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/100",
            "value": 64461,
            "range": "± 144",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_lines/500",
            "value": 344456,
            "range": "± 1481",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/draw_status",
            "value": 810,
            "range": "± 1805",
            "unit": "ns/iter"
          },
          {
            "name": "state_apply/menu_show_50",
            "value": 5687,
            "range": "± 29",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/80x24",
            "value": 47830,
            "range": "± 182",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/200x60",
            "value": 183010,
            "range": "± 468",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/full_frame/300x80",
            "value": 334153,
            "range": "± 672",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/500",
            "value": 3421960,
            "range": "± 12333",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/parse_apply_draw/1000",
            "value": 6769868,
            "range": "± 41983",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/80x24",
            "value": 10694,
            "range": "± 62",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/200x60",
            "value": 63741,
            "range": "± 661",
            "unit": "ns/iter"
          },
          {
            "name": "scaling/diff_incremental/300x80",
            "value": 126834,
            "range": "± 358",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_cold",
            "value": 11036,
            "range": "± 65",
            "unit": "ns/iter"
          },
          {
            "name": "view_cache/menu_select_warm",
            "value": 6147,
            "range": "± 154",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_cold",
            "value": 24568,
            "range": "± 395",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_warm",
            "value": 8034,
            "range": "± 12",
            "unit": "ns/iter"
          },
          {
            "name": "scene_cache_menu_select",
            "value": 19556,
            "range": "± 92",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_status_only",
            "value": 33126,
            "range": "± 279",
            "unit": "ns/iter"
          },
          {
            "name": "section_paint_menu_select",
            "value": 46113,
            "range": "± 527",
            "unit": "ns/iter"
          },
          {
            "name": "patch_status_update",
            "value": 5788,
            "range": "± 13",
            "unit": "ns/iter"
          },
          {
            "name": "patch_menu_select",
            "value": 6581,
            "range": "± 26",
            "unit": "ns/iter"
          },
          {
            "name": "patch_cursor_move",
            "value": 812,
            "range": "± 4",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_single_edit",
            "value": 13233,
            "range": "± 39",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_all_changed",
            "value": 11269,
            "range": "± 35",
            "unit": "ns/iter"
          },
          {
            "name": "apply_draw_line_comparison",
            "value": 14291,
            "range": "± 91",
            "unit": "ns/iter"
          },
          {
            "name": "line_dirty_buffer_status/1_line_changed",
            "value": 17739,
            "range": "± 14351",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/buffer_content/23_lines",
            "value": 2140,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/buffer_content/59_lines",
            "value": 5342,
            "range": "± 14",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/buffer_content/79_lines",
            "value": 7215,
            "range": "± 80",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/buffer_content/realistic_23",
            "value": 1484,
            "range": "± 21",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/buffer_cursor_only",
            "value": 229,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/status",
            "value": 208,
            "range": "± 1",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/menu/100_items",
            "value": 4437,
            "range": "± 11",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/all_flags/80x24",
            "value": 3146,
            "range": "± 5",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_sync_inputs/all_flags/300x80",
            "value": 8240,
            "range": "± 214",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/full_cold/salsa",
            "value": 41853,
            "range": "± 1588",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/full_cold/legacy",
            "value": 38644,
            "range": "± 313",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/menu_select_warm/salsa",
            "value": 41726,
            "range": "± 221",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/menu_select_warm/legacy",
            "value": 44585,
            "range": "± 216",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/incremental_edit/salsa",
            "value": 45410,
            "range": "± 81482",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_vs_legacy/incremental_edit/legacy",
            "value": 41368,
            "range": "± 7276",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_patched/status_update",
            "value": 1298,
            "range": "± 2",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_scene/cold",
            "value": 28834,
            "range": "± 26050",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_scene/warm",
            "value": 6426,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_scaling/full_frame/80x24",
            "value": 44141,
            "range": "± 1743",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_scaling/full_frame/200x60",
            "value": 142155,
            "range": "± 11861",
            "unit": "ns/iter"
          },
          {
            "name": "salsa_scaling/full_frame/300x80",
            "value": 226386,
            "range": "± 24730",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/80x24",
            "value": 123262,
            "range": "± 241",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw/200x60",
            "value": 704045,
            "range": "± 2733",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/incremental_1line",
            "value": 1754,
            "range": "± 6",
            "unit": "ns/iter"
          },
          {
            "name": "backend_draw/full_redraw_realistic/80x24",
            "value": 118957,
            "range": "± 582",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/80x24",
            "value": 40982,
            "range": "± 452",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw/200x60",
            "value": 219229,
            "range": "± 937",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/incremental_1line",
            "value": 12713,
            "range": "± 53",
            "unit": "ns/iter"
          },
          {
            "name": "draw_grid/full_redraw_realistic/80x24",
            "value": 40098,
            "range": "± 239",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_old",
            "value": 118373,
            "range": "± 256",
            "unit": "ns/iter"
          },
          {
            "name": "sgr_bytes/draw_grid_new",
            "value": 40081,
            "range": "± 105",
            "unit": "ns/iter"
          },
          {
            "name": "replay/normal_editing_50msg",
            "value": 3926234,
            "range": "± 9810",
            "unit": "ns/iter"
          },
          {
            "name": "replay/fast_scroll_100msg",
            "value": 16849042,
            "range": "± 33094",
            "unit": "ns/iter"
          },
          {
            "name": "replay/menu_completion_20msg",
            "value": 1782581,
            "range": "± 2527",
            "unit": "ns/iter"
          },
          {
            "name": "replay/mixed_session_200msg",
            "value": 19868099,
            "range": "± 62762",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/bg_instances_80x24",
            "value": 4744,
            "range": "± 47",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_hash_24rows",
            "value": 51212,
            "range": "± 154",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/row_spans_80cols",
            "value": 411,
            "range": "± 3",
            "unit": "ns/iter"
          },
          {
            "name": "gpu/color_resolve_1920cells",
            "value": 2312,
            "range": "± 14",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}