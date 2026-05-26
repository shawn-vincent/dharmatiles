// Rough-hewn block wall
// Fixed-size chamfered blocks
// Triangulated faces with inward-only random vertex cuts
// Random chips on all sides
// No bumps, no outward pulls

rows = 5;
cols = 6;

block_w = 24;
block_h = 12;
block_d = 10;

mortar = 1.2;
bevel = 0.8;

seed = 42;

sub_x = 6;
sub_y = 3;
sub_d = 3;

inset_max = 1.4;
jitter_frac = 0.28;

chips_per_block = 50;

$fn = 8;

function rnd(a,b,i) = rands(a,b,1,seed+i)[0];
function clamp(x,a,b) = min(max(x,a),b);

function is_edge(ix,iy,nx,ny) =
    ix == 0 || iy == 0 || ix == nx || iy == ny;

function safe_bevel(w,d,h,r) =
    min(r, min(w,d,h)/2 - 0.01);

function node_u(ix,iy,nx,ny,w,id,face) =
    let(step = w / nx, j = step * jitter_frac)
    is_edge(ix,iy,nx,ny)
        ? ix * step
        : clamp(
            ix * step + rnd(-j,j,id*100000 + face*10000 + ix*101 + iy*17),
            0,
            w
        );

function node_v(ix,iy,nx,ny,h,id,face) =
    let(step = h / ny, j = step * jitter_frac)
    is_edge(ix,iy,nx,ny)
        ? iy * step
        : clamp(
            iy * step + rnd(-j,j,id*110000 + face*10000 + ix*67 + iy*131),
            0,
            h
        );

function node_inset(ix,iy,nx,ny,id,face) =
    is_edge(ix,iy,nx,ny)
        ? 0
        : rnd(0,inset_max,id*120000 + face*10000 + ix*313 + iy*197);


// Exact outer dimensions: w x d x h
// Bevel changes only the chamfer depth, not the block size.
module beveled_box(w,d,h,r0) {
    r = safe_bevel(w,d,h,r0);
    hull()
        for (x=[r, w-r], y=[r, d-r], z=[r, h-r])
            translate([x,y,z]) sphere(r=r, $fn=8);
}


// Triangular prism cutter.
// Each triangle starts just outside a face and ends inward by vertex-specific inset.
module tri_cutter(face,w,d,h,p0,p1,p2,i0,i1,i2) {
    outer = 0.35;

    if (face == 0) { // front y=0, inward +Y
        polyhedron(
            points=[
                [p0[0], -outer, p0[1]],
                [p1[0], -outer, p1[1]],
                [p2[0], -outer, p2[1]],
                [p0[0], i0, p0[1]],
                [p1[0], i1, p1[1]],
                [p2[0], i2, p2[1]]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }

    if (face == 1) { // back y=d, inward -Y
        polyhedron(
            points=[
                [p0[0], d+outer, p0[1]],
                [p1[0], d+outer, p1[1]],
                [p2[0], d+outer, p2[1]],
                [p0[0], d-i0, p0[1]],
                [p1[0], d-i1, p1[1]],
                [p2[0], d-i2, p2[1]]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }

    if (face == 2) { // left x=0, inward +X
        polyhedron(
            points=[
                [-outer, p0[0], p0[1]],
                [-outer, p1[0], p1[1]],
                [-outer, p2[0], p2[1]],
                [i0, p0[0], p0[1]],
                [i1, p1[0], p1[1]],
                [i2, p2[0], p2[1]]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }

    if (face == 3) { // right x=w, inward -X
        polyhedron(
            points=[
                [w+outer, p0[0], p0[1]],
                [w+outer, p1[0], p1[1]],
                [w+outer, p2[0], p2[1]],
                [w-i0, p0[0], p0[1]],
                [w-i1, p1[0], p1[1]],
                [w-i2, p2[0], p2[1]]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }

    if (face == 4) { // bottom z=0, inward +Z
        polyhedron(
            points=[
                [p0[0], p0[1], -outer],
                [p1[0], p1[1], -outer],
                [p2[0], p2[1], -outer],
                [p0[0], p0[1], i0],
                [p1[0], p1[1], i1],
                [p2[0], p2[1], i2]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }

    if (face == 5) { // top z=h, inward -Z
        polyhedron(
            points=[
                [p0[0], p0[1], h+outer],
                [p1[0], p1[1], h+outer],
                [p2[0], p2[1], h+outer],
                [p0[0], p0[1], h-i0],
                [p1[0], p1[1], h-i1],
                [p2[0], p2[1], h-i2]
            ],
            faces=[[0,1,2],[3,5,4],[0,3,4,1],[1,4,5,2],[2,5,3,0]],
            convexity=4
        );
    }
}


module triangulated_face_cutters(face,w,d,h,nx,ny,id) {
    fw = (face == 2 || face == 3) ? d : w;
    fh = (face == 4 || face == 5) ? d : h;

    for (ix=[0:nx-1])
    for (iy=[0:ny-1]) {
        p00 = [node_u(ix,iy,nx,ny,fw,id,face),     node_v(ix,iy,nx,ny,fh,id,face)];
        p10 = [node_u(ix+1,iy,nx,ny,fw,id,face),   node_v(ix+1,iy,nx,ny,fh,id,face)];
        p01 = [node_u(ix,iy+1,nx,ny,fw,id,face),   node_v(ix,iy+1,nx,ny,fh,id,face)];
        p11 = [node_u(ix+1,iy+1,nx,ny,fw,id,face), node_v(ix+1,iy+1,nx,ny,fh,id,face)];

        i00 = node_inset(ix,iy,nx,ny,id,face);
        i10 = node_inset(ix+1,iy,nx,ny,id,face);
        i01 = node_inset(ix,iy+1,nx,ny,id,face);
        i11 = node_inset(ix+1,iy+1,nx,ny,id,face);

        diag = rnd(0,1,id*130000 + face*10000 + ix*211 + iy*431);

        if (diag < 0.5) {
            if (max(i00,i10,i11) > 0.01)
                tri_cutter(face,w,d,h,p00,p10,p11,i00,i10,i11);

            if (max(i00,i11,i01) > 0.01)
                tri_cutter(face,w,d,h,p00,p11,p01,i00,i11,i01);
        } else {
            if (max(i00,i10,i01) > 0.01)
                tri_cutter(face,w,d,h,p00,p10,p01,i00,i10,i01);

            if (max(i10,i11,i01) > 0.01)
                tri_cutter(face,w,d,h,p10,p11,p01,i10,i11,i01);
        }
    }
}


module chip_cutters(w,d,h,id) {
    for (i=[0:chips_per_block-1]) {
        face = floor(rnd(0,6,id*2000+i));

        x = face==0 ? rnd(-0.8,1.4,id*2000+i+10) :
            face==1 ? rnd(w-1.4,w+0.8,id*2000+i+11) :
            rnd(0,w,id*2000+i+12);

        y = face==2 ? rnd(-0.8,1.4,id*2000+i+20) :
            face==3 ? rnd(d-1.4,d+0.8,id*2000+i+21) :
            rnd(0,d,id*2000+i+22);

        z = face==4 ? rnd(-0.8,1.4,id*2000+i+30) :
            face==5 ? rnd(h-1.4,h+0.8,id*2000+i+31) :
            rnd(0,h,id*2000+i+32);

        translate([x,y,z])
            rotate([
                rnd(0,90,id*2000+i+40),
                rnd(0,90,id*2000+i+41),
                rnd(0,90,id*2000+i+42)
            ])
            cube([
                rnd(0.7,2.3,id*2000+i+50),
                rnd(0.7,2.3,id*2000+i+51),
                rnd(0.7,2.3,id*2000+i+52)
            ], center=true);
    }
}


module rough_block(w,h,d,id=0) {
    difference() {
        beveled_box(w,d,h,bevel);

        triangulated_face_cutters(0,w,d,h,sub_x,sub_y,id); // front
        triangulated_face_cutters(1,w,d,h,sub_x,sub_y,id); // back
        triangulated_face_cutters(2,w,d,h,sub_d,sub_y,id); // left
        triangulated_face_cutters(3,w,d,h,sub_d,sub_y,id); // right
        triangulated_face_cutters(4,w,d,h,sub_x,sub_d,id); // bottom
        triangulated_face_cutters(5,w,d,h,sub_x,sub_d,id); // top

        chip_cutters(w,d,h,id);
    }
}


module wall() {
    for (r=[0:rows-1]) {
        row_offset = (r % 2) * ((block_w + mortar) / 2);

        for (c=[0:cols-1]) {
            id = r*cols+c;

            translate([
                c*(block_w + mortar) - row_offset,
                0,
                r*(block_h + mortar)
            ])
            render()
                rough_block(block_w, block_h, block_d, id);
        }
    }
}

wall();