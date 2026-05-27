// DungeonBlocks - Blank Floor Tile

/* [Tile] */
cols = 1;  // number of tile columns
rows = 1;  // number of tile rows

/* [Wall] */
wall_n         = false;
wall_e         = false;
wall_w         = false;
wall_s         = false;
wall_height    = 33.0;  // height from slab base — top is fixed regardless of floor height
wall_thickness =  7.0;  // wall depth in mm

/* [Floor] */
floor_preset   = 9.5; // [0:Custom, 3:Water (3mm), 6:Ground (6mm), 9.5:Manmade (9.5mm)]
floor_custom   = 9.5; // custom height in mm — only used when preset is Custom
floor_texture  = true;
texture_depth  = 2.0; // how deep the texture is embossed in mm
texture_zoom   = 1.0; // 1 = full image, 2 = use 1/4 of image at 2x scale

/* [Socket] */
peg_height = 11.4; // [11.4:Normal, 5.7:Short]


/* [Hidden] */

tile_size        = 35.0;
peg_size         = 26.0;
corner_bevel     =  0.75;
round_corners    = true;
peg_flare_height = 5.2;
corner_fn        = 16;

floor_height = (floor_preset == 0) ? floor_custom : floor_preset;

peg_top_z = peg_height;
base_z    = peg_height + peg_flare_height;

total_w = cols * tile_size;
total_d = rows * tile_size;


// --- Helpers ---

module _corner_discs(size_xy, z, r) {
    inset = (tile_size - size_xy) / 2;
    rr    = min(r, size_xy / 2 - 0.01);
    for (x = [inset + rr, inset + size_xy - rr])
    for (y = [inset + rr, inset + size_xy - rr])
        translate([x, y, z])
            cylinder(r=rr, h=0.001, $fn=corner_fn);
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
                        sphere(r=rr, $fn=corner_fn);
                translate([inset, inset, z0 + height])
                    cube([size_xy, size_xy, 0.001]);
            }
        } else {
            hull() {
                for (x = [inset + rr, inset + size_xy - rr])
                for (y = [inset + rr, inset + size_xy - rr])
                    translate([x, y, z0])
                        cylinder(r=rr, h=0.001, $fn=corner_fn);
                translate([inset, inset, z0 + height])
                    cube([size_xy, size_xy, 0.001]);
            }
        }
    }
}

module square_frustum(bottom_size, top_size, z0, height, r=0) {
    if (r <= 0) {
        bi = (tile_size - bottom_size) / 2;
        ti = (tile_size - top_size) / 2;
        polyhedron(
            points = [
                [bi,             bi,             z0],
                [tile_size - bi, bi,             z0],
                [tile_size - bi, tile_size - bi, z0],
                [bi,             tile_size - bi, z0],
                [ti,             ti,             z0 + height],
                [tile_size - ti, ti,             z0 + height],
                [tile_size - ti, tile_size - ti, z0 + height],
                [ti,             tile_size - ti, z0 + height]
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
            _corner_discs(bottom_size, z0,          r);
            _corner_discs(top_size,    z0 + height, r);
        }
    }
}


// --- Components ---

module socket_base() {
    r           = round_corners ? corner_bevel : 0;
    bevel_entry = !round_corners && corner_bevel > 0;
    col_z0      = bevel_entry ? corner_bevel : 0;
    col_height  = peg_top_z - col_z0;

    if (bevel_entry)
        square_frustum(peg_size - 2 * corner_bevel, peg_size, 0, corner_bevel);

    centered_square_prism(peg_size, col_z0, col_height, r, round_corners);

    square_frustum(peg_size, tile_size, peg_top_z, peg_flare_height);
}

texture_file = "../textures/grass-foliage-256-faded.png";

module floor_tile() {
    translate([0, 0, base_z]) {
        if (floor_texture) {
            cube([total_w, total_d, floor_height - texture_depth]);
            translate([0, 0, floor_height - texture_depth])
                intersection() {
                    cube([total_w, total_d, texture_depth]);
                    resize([total_w * texture_zoom, total_d * texture_zoom, texture_depth])
                        surface(file=texture_file, invert=false);
                }
        } else {
            cube([total_w, total_d, floor_height]);
        }
    }
}

// --- Model ---

union() {
    for (c = [0:cols-1], r = [0:rows-1])
        translate([c * tile_size, r * tile_size, 0])
            socket_base();
    floor_tile();
    if (wall_w) translate([0,                        0, base_z]) cube([wall_thickness, total_d, wall_height]);
    if (wall_e) translate([total_w - wall_thickness, 0, base_z]) cube([wall_thickness, total_d, wall_height]);
    if (wall_s) translate([0,                        0, base_z]) cube([total_w, wall_thickness, wall_height]);
    if (wall_n) translate([0, total_d - wall_thickness, base_z]) cube([total_w, wall_thickness, wall_height]);
}
