# the notes the page prints: stages, the rule table and the departures. OVER holds any numbers changed from wall.js's defaults.
OVER = {'wholeShare': .5, 'crackShare': .25, 'crackCuts': [1, 2], 'skinSetback': [.2, .8], 'skinTilt': [.4, 1.6]}
CLOSE_CROP = (200, 20, 200)
STAGES = [
 (1, '1 Mass', 'Three volumes stepping up and back, silhouette only.'),
 (2, '2 Primary planes and breaks', 'Courses, joints, depth push, the recess with the course above set forward, cap blocks.'),
 (3, '3 Skin over core', 'Some faces split once into a skin piece set back and turned a little.'),
 (4, '4 Edges', 'Seven in ten soft, three crisp; top-front bevel; vertical front edges.'),
 (5, '5 Chips', 'Corner chips on the top corners, 0 to 2 a block.'),
 (6, '6 Cracks', 'Gaps cut inside a face, each ending inside its parent.'),
 (7, '7 Foot and rubble', 'Foot stair, rubble strip, the figure.'),
]
TABLE = [
 ('1 Mass: volumes cover 0.52-0.60 of depth', 'wall y 14-35 (0.6 L), figure 0.4 L', 'as written', 'Front space for the figure is right.'),
 ('1 Mass: wall height H 38-43, courses 11.5-14.5', 'courses 9-11, H about 31 + caps 3-5', 'changed', 'At 38-43 the wall stood about 1.3 times too tall against the plate in the photo.'),
 ('1 Mass: boxes stepping up and back, .26/.52/.80 H', 'three courses stepping back 3.0 mm each, two columns of height', 'changed', 'Read by depth (front low, back high), not by column. By column made a tower.'),
 ('2 Blocks 12-22 wide, joints every ~14', 'width 12-22, joint 0.5', 'as written', ''),
 ('2 Joints repeat below 1 in 3, else stagger 3.2', 'carry 0.35, stagger 3.2', 'as written', ''),
 ('2 Depth push 0-6; recess 6-8, above forward 2-3', 'push 0-6; recess 6-8; above -2..-3 + 3.0', 'as written', 'Needs the back of the block to stay 2.4 in front of the core.'),
 ('3 Skin: 60% whole, rest split once', 'whole 0.6; setback 0.2-0.8; tilt 0.2-1.2', 'as written', ''),
 ('4 Edges: 70% soft 0.7-2.8, top-front 1.6-3.4', 'as written', 'as written', ''),
 ('5 Chips: 0-2 a block, 0.9-2.8 deep', 'as written', 'as written', ''),
 ('6 Cracks 0.4-0.7 wide, long, branch', 'gap 0.45, inside a face', 'changed', 'A half-space cell cannot make a crack that branches or reaches the edge; see departures.'),
 ('7 Foot: 11-14 blocks 3-6, a strip', '12 blocks 3-6 and two stair blocks', 'as written', ''),
]
DEPARTURES = [
 ('Wall height.', 'The recipe says H 38-43 with courses of 11.5-14.5. Against the plate in the photo the wall stood about 1.3 times too tall, so courses are 9-11 and H is about 31 plus caps of 3-5. The 38-43 is the sheet height (27 mm times 1.4) and does not hold for the panel.'),
 ('Tiers by depth, not by column.', 'Heights .26/.52/.80 H with footprints .20/.60/.20 L read as three towers. The photo is a stair seen from the front: each course sits further back (3 mm a course) and the right column stands 1.5 mm proud. The recipe should say which axis the tiers step along.'),
 ('Far more pieces than 40% split once.', 'A face split once in 40% of blocks looked like bare boxes beside the photo, where nearly every face is cut into polygons. I used 70% split, then 3 to 6 more cuts in 85% of the blocks, each cut living inside one cell, gap 0.45, setback 0.3 to 1.0, tilt 0.5 to 2 degrees. The recipe&rsquo;s &ldquo;2 to 5 pieces read as rubble&rdquo; held only when the gaps were deep and the tilts large; with a shallow gap it reads as hewn.'),
 ('Cracks as relief, not cells (round 2).', 'The half-space cell cannot make a crack that branches or runs to an edge. Round 2 cuts each front, top and left face into a fine mesh (0.3 mm) and sets its height from an irregular stretched cell grid: a facet plane per cell (up to 2.5 degrees), a V groove on 45 to 60% of cell borders (0.3 to 0.7 deep, 1.7 wide, so borders branch and reach the block edge), a 0.12 step across a groove, and clusters of 3 to 5 pits. The height is zero on the face outline, so the solid stays closed. Grooves are 1.7 wide, not 0.4 to 0.7: at 0.3 mm sampling a narrower V aliases into ladders. The recipe asks for half-spaces only, and that cannot reach this look.'),
 ('Pits.', 'A pit is a displaced dip (radius 0.45 to 0.95, depth 0.3 to 0.6) in a cluster, not a plane intersection. Small pebbles (0.45 to 0.95) stand on the tops. Both are my own additions to step 6.'),
 ('Cap bevels.', 'A top-front bevel of 1.6 to 3.4 on a cap 3 to 5 high turned the caps into loaves. Bevels are limited to 0.3 of the block height.'),
 ('Lighting numbers.', 'The page&rsquo;s own weights (hemisphere 0.6, key from front left) were right in shape. Surface grey 0.8 with a 0.62 hemisphere and a 0.5 key matched the photo; at grey 0.95 it burned out, at 0.72 it read dark and flat.'),
 ('Not reachable with half-spaces.', 'Pits and dishes (a plane cut into a face removes the whole half of it), cracks that branch and reach an edge, and the hewn fine facets of the photo. Cracks here are gaps between cells; pits are left out. This is the biggest remaining difference.'),
 ('Rubble.', 'Eleven to fourteen blocks in a strip held, but the photo scatters small stones everywhere over the plate, including round the figure&rsquo;s base. Round 2 adds 46 more (22% blocks 2.5 to 5, the rest pebbles 1.2 to 3) outside the figure&rsquo;s base and the wall footprint. The recipe&rsquo;s &ldquo;strip along the foot&rdquo; undercounts them.'),
 ('Layout.', 'The wall now starts at x = 8 so the figure (base radius 5.8 at 5.8, 7.5) stands at the front left beside it; blocks are 9 to 15 wide, not 12 to 22, because the wall is 27 wide, not 32. The literal run (v1_literal_whole.png, ?literal=1 on the page) uses the recipe as written: columns, H 38-43, one skin split, no relief, a strip of rubble.'),
]
