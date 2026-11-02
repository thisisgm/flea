//! G1.07 Columns view: the model, geometry, peeks and track Elements, each check naming the v0.3.12 source line it ports.
#[path = "../../../src/i18n/test_catalogs.rs"]
mod test_catalogs;
#[path = "g1_07/text_oracle.rs"]
mod text_oracle;
// test_catalogs reaches the catalog types through super.
use flea::i18n::{I18n, Paths};
#[path = "g1_07/board.rs"]
mod board;
#[path = "g1_07/check.rs"]
mod check;
#[path = "g1_07/cols_back_qml.rs"]
mod cols_back_qml;
#[path = "g1_07/cols_clip.rs"]
mod cols_clip;
#[path = "g1_07/cols_cost_prebuild.rs"]
mod cols_cost_prebuild;
#[path = "g1_07/cols_cost_prebuild_2.rs"]
mod cols_cost_prebuild_2;
#[path = "g1_07/cols_dividers_narrow.rs"]
mod cols_dividers_narrow;
#[path = "g1_07/cols_drawn_held.rs"]
mod cols_drawn_held;
#[path = "g1_07/cols_filter.rs"]
mod cols_filter;
#[path = "g1_07/cols_filter_2.rs"]
mod cols_filter_2;
#[path = "g1_07/cols_folder.rs"]
mod cols_folder;
#[path = "g1_07/cols_folder_2.rs"]
mod cols_folder_2;
#[path = "g1_07/cols_geom.rs"]
mod cols_geom;
#[path = "g1_07/cols_hairlines.rs"]
mod cols_hairlines;
#[path = "g1_07/cols_hidden.rs"]
mod cols_hidden;
#[path = "g1_07/cols_moves.rs"]
mod cols_moves;
#[path = "g1_07/cols_moves_2.rs"]
mod cols_moves_2;
#[path = "g1_07/cols_moves_3.rs"]
mod cols_moves_3;
#[path = "g1_07/cols_moves_4.rs"]
mod cols_moves_4;
#[path = "g1_07/cols_peekdrag.rs"]
mod cols_peekdrag;
#[path = "g1_07/cols_peekgate_order.rs"]
mod cols_peekgate_order;
#[path = "g1_07/cols_pwid.rs"]
mod cols_pwid;
#[path = "g1_07/columns_back.rs"]
mod columns_back;
#[path = "g1_07/columns_back_2.rs"]
mod columns_back_2;
#[path = "g1_07/columns_back_3.rs"]
mod columns_back_3;
#[path = "g1_07/columns_back_4.rs"]
mod columns_back_4;
#[path = "g1_07/columns_js_a.rs"]
mod columns_js_a;
#[path = "g1_07/columns_js_a_2.rs"]
mod columns_js_a_2;
#[path = "g1_07/columns_js_a_3.rs"]
mod columns_js_a_3;
#[path = "g1_07/columns_js_a_4.rs"]
mod columns_js_a_4;
#[path = "g1_07/columns_js_a_5.rs"]
mod columns_js_a_5;
#[path = "g1_07/columns_js_b.rs"]
mod columns_js_b;
#[path = "g1_07/columns_js_b_2.rs"]
mod columns_js_b_2;
#[path = "g1_07/columns_js_b_3.rs"]
mod columns_js_b_3;
#[path = "g1_07/columns_js_b_4.rs"]
mod columns_js_b_4;
#[path = "g1_07/columns_js_b_5.rs"]
mod columns_js_b_5;
#[path = "g1_07/columns_js_b_6.rs"]
mod columns_js_b_6;
#[path = "g1_07/columns_keep.rs"]
mod columns_keep;
#[path = "g1_07/elements.rs"]
mod elements;
#[path = "g1_07/fixture.rs"]
mod fixture;
#[path = "g1_07/frames.rs"]
mod frames;
#[path = "g1_07/ipc.rs"]
mod ipc;
#[path = "g1_07/pwid_legs.rs"]
mod pwid_legs;
#[path = "g1_07/smoke.rs"]
mod smoke;
#[path = "g1_07/track_frames.rs"]
mod track_frames;
#[path = "g1_07/track_frames_2.rs"]
mod track_frames_2;
#[path = "g1_07/track_frames_3.rs"]
mod track_frames_3;
#[path = "g1_07/track_frames_4.rs"]
mod track_frames_4;
#[path = "g1_07/track_frames_5.rs"]
mod track_frames_5;

#[path = "g1_07/theme.rs"]
mod theme;
