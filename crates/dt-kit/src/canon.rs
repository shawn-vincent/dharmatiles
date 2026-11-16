//! The kit's types in the canonical form (`dt_value::Canon`): what a cache
//! key, a step's or a thing's, reads of a frame and a printer.

use dt_value::{Canon, Canonical};

use crate::{Frame, Printer};

impl Canonical for Frame {
    fn canon(&self, c: &mut Canon) {
        let Self {
            cols,
            rows,
            square_mm,
            cells_per_square,
        } = self;
        c.tag(b'F')
            .put(cols)
            .put(rows)
            .put(square_mm)
            .put(cells_per_square);
    }
}

impl Canonical for Printer {
    fn canon(&self, c: &mut Canon) {
        let Self {
            overhang_deg,
            steepest_deg,
            layer_mm,
            line_mm,
            gap_mm,
            reach_mm,
            span_mm,
            grid_mm,
            overhang_budget_mm2,
        } = self;
        c.tag(b'P')
            .put(overhang_deg)
            .put(steepest_deg)
            .put(layer_mm)
            .put(line_mm)
            .put(gap_mm)
            .put(reach_mm)
            .put(span_mm)
            .put(grid_mm)
            .put(overhang_budget_mm2);
    }
}
