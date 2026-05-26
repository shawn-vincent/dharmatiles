//
// DungeonBlocks floor + one-wall tile
// Clean parametric reconstruction
//

// ---------- Tile footprint ----------
tile_size = 35.0;


// ---------- Socket (underside peg) ----------
socket_height       = 16.6;  // total socket height = tile base z
socket_mid_size     = 26.0;  // column cross-section
socket_flare_height = 5.2;   // structural top flare, always sharp-cornered

corner_size      = 1.5;      // bevel depth or fillet radius on column corners; 0 = none
corner_round     = false;    // false = chamfer bevel, true = curved fillet
socket_corner_fn = 16;       // $fn for fillet cylinders/spheres

logo_S           = 1.5;      // size unit; logo width = 10*logo_S
logo_stroke      = 0.4;      // stroke width in mm (independent of size)
logo_fn          = 64;       // $fn for logo circles/arcs
logo_circle_r    = 0.8;      // central circle radius multiplier (× logo_S)
logo_petal_r     = 4.0;      // petal arc radius in mm (smaller = more curved)
logo_squash      = 0.65;     // vertical squash of top face (0.5 = isometric)
logo_box_depth   = 3.0;      // side-wall drop in mm


// ---------- Terrain block (floor slab) ----------
tile_bulk_height   = 8.0;
tile_detail_height = 1.5;

floor_detail_x0 = 6.0;
floor_detail_x1 = 34.0;
floor_detail_y0 = 1.0;
floor_detail_y1 = 34.0;


// ---------- Wall ----------
wall_height    = 33.0;
wall_thickness = 6.0;


// ---------- Derived Z positions (not parameters) ----------
socket_top_z = socket_height - socket_flare_height;
tile_base_z  = socket_height;

tile_bulk_top_z   = tile_base_z    + tile_bulk_height;
tile_detail_top_z = tile_bulk_top_z + tile_detail_height;

wall_bottom_z = tile_base_z;
wall_top_z    = wall_bottom_z + wall_height;

wall_x0 = 0.0;
wall_x1 = wall_thickness;
wall_y0 = 0.0;
wall_y1 = tile_size;


// ---------- Helpers ----------

module _socket_corner_discs(size_xy, z, r) {
    inset = (tile_size - size_xy) / 2;
    rr    = min(r, size_xy / 2 - 0.01);
    for (x = [inset + rr, inset + size_xy - rr])
    for (y = [inset + rr, inset + size_xy - rr])
        translate([x, y, z])
            cylinder(r=rr, h=0.001, $fn=socket_corner_fn);
}

module centered_square_prism(size_xy, z0, height, r=0, round_bottom=false) {
    inset = (tile_size - size_xy) / 2;
    if (r <= 0) {
        translate([inset, inset, z0])
            cube([size_xy, size_xy, height], center=false);
    } else {
        rr = min(r, size_xy / 2 - 0.01);
        if (round_bottom) {
            hull() {
                for (x = [inset + rr, inset + size_xy - rr])
                for (y = [inset + rr, inset + size_xy - rr])
                    translate([x, y, z0 + rr])
                        sphere(r=rr, $fn=socket_corner_fn);
                translate([inset, inset, z0 + height])
                    cube([size_xy, size_xy, 0.001]);
            }
        } else {
            hull() {
                for (x = [inset + rr, inset + size_xy - rr])
                for (y = [inset + rr, inset + size_xy - rr])
                    translate([x, y, z0])
                        cylinder(r=rr, h=0.001, $fn=socket_corner_fn);
                translate([inset, inset, z0 + height])
                    cube([size_xy, size_xy, 0.001]);
            }
        }
    }
}

module square_frustum(bottom_size, top_size, z0, height, r=0) {
    if (r <= 0) {
        bottom_inset = (tile_size - bottom_size) / 2;
        top_inset    = (tile_size - top_size) / 2;
        polyhedron(
            points = [
                [bottom_inset,             bottom_inset,             z0],
                [tile_size - bottom_inset, bottom_inset,             z0],
                [tile_size - bottom_inset, tile_size - bottom_inset, z0],
                [bottom_inset,             tile_size - bottom_inset, z0],
                [top_inset,                top_inset,                z0 + height],
                [tile_size - top_inset,    top_inset,                z0 + height],
                [tile_size - top_inset,    tile_size - top_inset,    z0 + height],
                [top_inset,                tile_size - top_inset,    z0 + height]
            ],
            faces = [
                [0, 3, 2, 1],
                [4, 5, 6, 7],
                [0, 1, 5, 4],
                [1, 2, 6, 5],
                [2, 3, 7, 6],
                [3, 0, 4, 7]
            ]
        );
    } else {
        hull() {
            _socket_corner_discs(bottom_size, z0,          r);
            _socket_corner_discs(top_size,    z0 + height, r);
        }
    }
}


// ---------- Logo ----------

// Arc center on the inward (origin-side) perpendicular bisector of chord p1→p2
// at radius r. The resulting arc bows outward away from the origin.
function _petal_arc_center(p1, p2, r) =
    let(mx = (p1[0]+p2[0])/2,  my = (p1[1]+p2[1])/2,
        dx =  p2[0]-p1[0],     dy =  p2[1]-p1[1],
        d  = sqrt(dx*dx + dy*dy),
        h  = sqrt(max(0, r*r - d*d/4)),
        nx = -dy/d,            ny =  dx/d,
        s  = (nx*mx + ny*my > 0) ? -1 : 1)
    [mx + s*h*nx,  my + s*h*ny];

// Signed angular span a1→a2 via the short arc, result in (-180, 180].
function _short_span(a1, a2) =
    let(raw = a2 - a1,
        n   = raw - 360*floor(raw/360))
    (n > 180) ? n - 360 : n;

// Filled stroked-arc polygon: center c, radius r, start a1, angular span da.
module _arc_stroke_2d(c, r, a1, da, st) {
    ro = r + st/2;
    ri = r - st/2;
    polygon(concat(
        [for (i=[0:logo_fn]) let(a=a1+i*da/logo_fn) [c[0]+ro*cos(a), c[1]+ro*sin(a)]],
        [for (i=[logo_fn:-1:0]) let(a=a1+i*da/logo_fn) [c[0]+ri*cos(a), c[1]+ri*sin(a)]]
    ));
}

// Stroked arc from p1 to p2 bowing outward from the origin.
module _petal_arc_stroke(p1, p2, rp, st) {
    c  = _petal_arc_center(p1, p2, rp);
    a1 = atan2(p1[1]-c[1], p1[0]-c[0]);
    a2 = atan2(p2[1]-c[1], p2[0]-c[0]);
    _arc_stroke_2d(c, rp, a1, _short_span(a1, a2), st);
}

// Rotate point p by rot degrees then squash y by sq.
function _sq_rot(p, rot, sq) =
    let(c = cos(rot),  s = sin(rot),
        rx = c*p[0] - s*p[1],
        ry = s*p[0] + c*p[1])
    [rx, ry*sq];

// Stroked straight line between two 2D points.
module _logo_line_stroke(p1, p2, st) {
    dx = p2[0]-p1[0];  dy = p2[1]-p1[1];
    translate([(p1[0]+p2[0])/2, (p1[1]+p2[1])/2])
        rotate([0, 0, atan2(dy, dx)])
        square([sqrt(dx*dx+dy*dy), st], center=true);
}

module dharma_logo_2d(S, st) {
    tw      = 10 * S;
    half    = tw / 2;
    rc      = logo_circle_r * S;
    rp      = logo_petal_r;
    sq      = logo_squash;
    dep     = logo_box_depth;
    ps      = rc + (half - rc) / 6;
    pt      = (half - st) / 2;
    hs      = half * sq;   // squashed half-height (y of front/back diamond corners)
    sq_poly = [[0, hs], [half, 0], [0, -hs], [-half, 0]];

    // Rotate so the box "vertical" runs along the tile X axis, then centre.
    rotate([0, 0, -90])
    translate([0, dep/2]) {
        // Diamond outline — offset on squashed polygon gives constant-width stroke
        difference() {
            polygon(sq_poly);
            offset(delta=-st) polygon(sq_poly);
        }

        // Cross arms in squashed coordinates, constant stroke width:
        //   horizontal bar unchanged; vertical bar height = tw*sq; ellipse cutout
        intersection() {
            polygon(sq_poly);
            difference() {
                union() {
                    square([st,  tw*sq], center=true);  // vertical arm, squashed
                    square([tw,  st   ], center=true);  // horizontal arm
                }
                scale([1, sq]) circle(r=rc-st/2, $fn=logo_fn);  // ellipse cutout
            }
        }

        // Central circle → ellipse; offset() gives constant-width stroke around it
        difference() {
            offset(delta=+st/2) scale([1, sq]) circle(r=rc, $fn=logo_fn);
            offset(delta=-st/2) scale([1, sq]) circle(r=rc, $fn=logo_fn);
        }

        // Petals: rotate then squash each endpoint so arcs live in squashed space
        intersection() {
            polygon(sq_poly);
            for (rot = [0:90:270]) {
                p1 = _sq_rot([0,  ps], rot, sq);
                p2 = _sq_rot([ps,  0], rot, sq);
                pm = _sq_rot([pt, pt], rot, sq);
                _petal_arc_stroke(p1, pm, rp, st);
                _petal_arc_stroke(p2, pm, rp, st);
            }
        }

        // Box side walls: 3 vertical edges + 2 bottom connecting edges
        _logo_line_stroke([-half,   0],  [-half,  -dep],    st);
        _logo_line_stroke([ half,   0],  [ half,  -dep],    st);
        _logo_line_stroke([    0, -hs],  [    0, -hs-dep],  st);
        _logo_line_stroke([-half, -dep], [    0, -hs-dep],  st);
        _logo_line_stroke([    0, -hs-dep], [half, -dep],   st);
    }
}


// ---------- Components ----------
module socket_base() {
    r           = corner_round ? corner_size : 0;
    bevel_entry = !corner_round && corner_size > 0;
    col_z0      = bevel_entry ? corner_size : 0;
    col_height  = socket_top_z - col_z0;

    difference() {
        union() {
            // Bottom entry: linear chamfer or spherical-cap roundover
            if (bevel_entry)
                square_frustum(
                    socket_mid_size - 2 * corner_size,
                    socket_mid_size,
                    0,
                    corner_size
                );

            centered_square_prism(
                socket_mid_size, col_z0, col_height,
                r, corner_round
            );

            square_frustum(socket_mid_size, tile_size, socket_top_z, socket_flare_height);
        }

        // Logo inset 0.4 mm into the bottom face
        translate([tile_size/2, tile_size/2, -0.01])
            linear_extrude(height=0.41)
                dharma_logo_2d(logo_S, logo_stroke);
    }
}

module floor_tile() {
    union() {
        translate([0, 0, tile_base_z])
            cube([tile_size, tile_size, tile_bulk_height], center=false);

        translate([floor_detail_x0, floor_detail_y0, tile_bulk_top_z])
            cube([
                floor_detail_x1 - floor_detail_x0,
                floor_detail_y1 - floor_detail_y0,
                tile_detail_height
            ], center=false);
    }
}

module one_wall() {
    translate([wall_x0, wall_y0, wall_bottom_z])
        cube([
            wall_x1 - wall_x0,
            wall_y1 - wall_y0,
            wall_top_z - wall_bottom_z
        ], center=false);
}


// ---------- Final model ----------
union() {
    socket_base();
    floor_tile();
    one_wall();
}
