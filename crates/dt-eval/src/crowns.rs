//! A tree's `top=` worked out where its branch tips stand: the formula is
//! read like any top (`$x $y $d $a $bottom $region`), with `$region` the
//! tree's own disc and `parent($region)` the region it stands in, and gives
//! a cloud at each tip. The tree rolls its tips inside it.

use dt_elements::{CrownTops, LookError, Tip, TopCloud};
use dt_geom::Iv;
use dt_kit::Frame;

use crate::coords::Coords;
use crate::doc::Tile;
use crate::regions::RegionTree;
use crate::solve::{Disc, Eval, Vars};

/// The formulas of a tile's trees, over its regions.
pub(crate) struct Crowns<'a> {
    pub(crate) tile: &'a Tile,
    pub(crate) regions: Option<&'a RegionTree>,
    pub(crate) coords: Option<Coords<'a>>,
    pub(crate) frame: Frame,
}

impl CrownTops for Crowns<'_> {
    fn cloud(&self, id: usize, tip: &Tip) -> Result<TopCloud, LookError> {
        let Some(top) = self.tile.tree_tops.get(id) else {
            return Err(LookError {
                control: None,
                message: format!("a tree's top (number {id}) is not in the tile's table"),
                help: None,
                code: None,
            });
        };
        let own = self.regions.map_or(0, |t| t.owner(top.owner));
        let eval = Eval {
            table: &top.table,
            coords: self.coords.as_ref().map(|c| (c, own)),
            disc: Some(Disc {
                centre: tip.centre,
                radius: tip.radius,
            }),
        };
        let (nx, ny) = self.frame.nodes();
        let per = f64::from(self.frame.cells_per_square);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (i, j) = (
            ((tip.at[0] * per).round().max(0.0) as usize).min(nx - 1),
            ((tip.at[1] * per).round().max(0.0) as usize).min(ny - 1),
        );
        let vars = Vars {
            x: Iv::point(tip.at[0]),
            y: Iv::point(tip.at[1]),
            bottom: Iv::point(tip.bottom_dmm),
            k: j * nx + i,
        };
        match eval.iv(&top.expr, &vars, 0) {
            Ok(v) => Ok(TopCloud {
                lo: v.iv.lo,
                hi: v.iv.hi,
                odds: v.odds,
            }),
            Err(f) => Err(LookError {
                control: Some("top".to_owned()),
                message: f.message,
                help: f.help,
                code: Some(f.code),
            }),
        }
    }
}
