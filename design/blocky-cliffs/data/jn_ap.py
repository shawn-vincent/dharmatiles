# Hand-traced joints, in percent of the cell crop (x right, y down), over the same crops as tr_ap.py.
# each joint: stone a, stone b, kind (col course cap lean other), the contact's two ends (T skyline, G ground, S side, J junction, H hidden),
#   the contact as parts 'v pts | w pts | f pts' (v a line shows, w a faint line, f no line; each part starts where the last ended),
#   flags 'rank fade dies two conf': rank F front, B back, S side; fade g gradual, s stops; dies f foot, b back, m mid-face, t top;
#   two y/n: where merged, does the outline still show two stones; conf h m l; and a note.
ADDED=[]
J={}
J['A']=[
('1','2','col','J','J','v 66,50 65.5,52.5 64.5,58 63.5,64.5','F - - - h','groove down the front, crisp'),
('3','1','cap','J','S','v 66,50 72,47 89,44.5','B - - - h',''),
('3','2','cap','T','J','v 60,46.5 66,50','B - - - h',''),
('4','1','other','J','G','w 63.5,64.5 62.5,71 62,77','F - - - m','toe against the block: softer toward the plate'),
('4','2','other','J','G','v 63.5,64.5 60,64.5 57,66 | w 57,72','F - - - m','the toe is a soft swelling at the foot of the joint'),
]
J['B']=[
('1','2','col','J','G','v 72,40 71,50 70.5,62 | w 70,77','F - - - m','wide at the head, a hairline at the foot; it reaches the plate'),
('2','3','col','J','G','v 60.5,41.5 57,52 55.5,66 | w 54.5,74','F - - - m','reaches the plate, faint in the last tenth'),
('4','3','col','T','J','v 54,14 60,17 66,20 66,35 62,39.5','B - - - h','deep shadowed re-entrant'),
('4','2','cap','J','J','v 62,39.5 64,38.5 70,38.5 72,40','F - - - h',''),
('4','1','cap','J','S','v 72,40 76,37 87,35 92,37','F - - - h',''),
]
J['C']=[
('2','1','other','J','G','v 64,61.5 60,72 58.5,74.5','F - - - h',''),
('3','2','course','S','J','v 52,58 60,60 64,61.5','F - - - h',''),
('3','1','course','J','J','v 64,61.5 65,59 70,55 76,52','F - - - h',''),
('4','1','course','J','S','v 76,52 80,53 88,53','F - - - h',''),
('4','3','col','S','J','v 67,42 76,43 | w 76,52','B - - - m','a line along the top of 3, a faint one down its end'),
('5','3','course','S','J','v 59,41.5 67,42','B - - - m',''),
('5','4','col','J','J','v 67,42 68,32.5 71,30 77.5,29','B - - - m',''),
('6','5','col','S','J','v 67,16.5 73,16 77,17.5 | f 77.5,29','B s m y m','5 is a step on the flank of 6: its top edge is a line, below it one face'),
('6','4','course','J','S','v 77.5,29 82,28.5 84.5,29','B - - - m',''),
]
J['D']=[
('1','2','col','J','G','v 70,44.5 63,60 61.5,68 | w 60,75','F - - - h',''),
('2','3','col','J','G','v 58,45 53,57 50,71','F - - - h',''),
('5','1','cap','J','S','v 71,41 75,37 84,33.5 92,32','F - - - h',''),
('5','2','cap','J','J','v 61,42.5 68,43.5 70.5,44','F - - - h',''),
('3','5','col','J','J','v 64,20.5 66,22.5 64,40 61,42.5','B - - - h','the deepest groove on the sheet'),
('4','3','col','T','H','f 45,21.5 37,24.5 33,27 | w 33,38','B g t y l','one top surface across both; a faint line down the front only'),
('6','3','cap','S','J','f 34,19.5 40,20.5 | w 45,21.5 | v 58,20','B g b y l','the slab sinks into the top toward the far end'),
('7','6','cap','S','J','v 50,15.5 58,16.5 58,20','B - - - m',''),
('7','5','cap','J','J','v 58,20 63.5,20.5 | w 66,18.5','B - - - m',''),
('7','8','cap','T','J','v 70,11.5 70,14 66,18.5','B - - - m','a notch'),
('8','5','cap','J','S','v 66,18.5 72,17 88,15.5 94,16','B - - - h',''),
]
J['E']=[
('2','1','course','S','S','v 40,62.5 44,61.5 48,63 79,70.5 | f 94.5,65.5','F s e n h','crisp along the front; on the cut end the two courses are one face'),
('3','2','course','S','S','v 48,45.5 72,47.5 88,47.5 92,46','F - - - h',''),
('4','3','course','S','S','v 68,29 80,30 88,30.5 91,33','B - - - h',''),
('2a','2b','col','J','J','f 66,47.5 65,52.5 | w 63,60 61,66.5','F s b y m','a crack in the riser of the second course; nothing shows across the tread behind it'),
]
ADDED+=[('E2a','E2','left of the crack','the second course is two stones'),('E2b','E2','right of the crack','')]
J['F']=[
('1','2','col','J','G','v 69,35 68,40 65.5,60 | w 64.5,68 63.5,76','F - - - h',''),
('2','3','col','J','G','v 53,41 52,50 51.5,62 | w 51,72','F - - - m',''),
('3','4','col','J','G','w 45,42 44,52 | f 43.5,62 44,71','F g f n l','the small pillar is a swelling on the face of the wall: no line in its lower half'),
('4','5','col','T','J','v 60,11 59.5,20 59,30 57,34','B - - - h','a crack from the skyline down to the top of the pillar in front, which hides the rest'),
('5','1','col','S','J','v 93,15 88,15.5 76,19.5 72,23 68,33 69,35','B - - - h',''),
('5','2','cap','J','J','v 57,34 58,35.5 66,34.5 69,35','F - - - m',''),
('4','2','col','J','J','v 57,34 55,38 53,41','F - - - m',''),
]
J['G']=[
('1','2a','other','J','G','v 77,48 62,52 57,56.5 53,64 50,73','F - - - h',''),
('1','2b','other','S','J','v 89,50 84,48.5 77,48','F - - - h',''),
('2a','2b','lean','T','J','v 78,22 77.5,30 77,40 | w 77,48','B - - - m','a crease more than a groove'),
('3','2a','cap','S','J','v 54,28 64,27 70,24.5','B - - - m',''),
('4','2a','cap','J','J','v 70,24.5 78,22','B - - - m',''),
('4','2b','cap','J','S','w 78,22 84,21','B - - - l',''),
('4','3','col','S','J','v 60.5,14 68,14.5 71,16 70,24.5','B - - - h',''),
]
ADDED+=[('G2a','G2','left of the crease','the leaning mass is two slabs'),('G2b','G2','right of the crease','')]
J['H']=[
('3','4','col','T','J','v 65.5,10 65,20 64,30 62,33.5 60,36','B - - - h','the cleft: open'),
('2','3','col','J','G','v 60,36 58,46 56,55 55,66 54,74','F - - - h','faint in the last third, and it reaches the plate'),
('2','4','col','J','J','v 63,33 74,31.5 84,30 86.5,32 85,35 | f 84,37 84,53','F s m y m',''),
('1','2','other','J','G','v 84,53 80,56 79,62 76,78','F - - - h',''),
('1','4','other','J','S','v 84,53 89,52.5 92,56','F - - - m',''),
]
J['I']=[
('2','1','course','J','S','v 44,59 70,62.5 88,61 92,64','F - - - h',''),
('1','3','other','J','G','v 44,59 41,60 | f 42,71','F g f n l','behind the shield'),
('2','3','other','J','J','v 45,59 45,47 48,45 53,44','F - - - m',''),
('4','2','course','J','S','v 53,44 62,43 88,42 93,45','F - - - h',''),
('3','4','other','T','J','v 49,29 52,33 53,37.5','B - - - m',''),
('5','4','course','S','S','v 66,18.5 68,18 88,19.5 90,20.5','B - - - h',''),
]
J['J']=[
('1','3','other','S','G','v 91,46 84,49 79,50 76,53 73,60 | w 71,74','F - - - h',''),
('3','4','col','S','G','v 73,20 66,19.5 63,22 62,30 | w 59,50 60,69','F - - - m',''),
('5','4','other','T','S','v 55,10 54,12.5 54,22 53,27','B - - - h','the wedged stone over the niche'),
('5','6','other','T','S','v 39,13 40,14 43,17 44,26','B - - - h',''),
]
J['K']=[
('1','2','other','J','G','v 75,52.5 84,51.5 89,52.5 91,56 | f 93,70.5 94,78','S s f n m',''),
('1','6','other','J','G','v 77,52 70,53 66,56 63,62 60,74','F - - - h',''),
('3','2','cap','J','S','v 81,40.5 84,39 93,37.5','F - - - h',''),
('2','6','col','J','J','v 77.5,46 77,47 77,52.5','B - - - m',''),
('3','4','cap','J','S','v 59.5,17.5 58,19 58,32 63,35.5','F - - - h','the joint between the two roof stones: open top to soffit'),
('5','3','cap','J','T','v 59,18 62,15 72,12 82,9','B - - - m',''),
('3','6','cap','S','J','v 62,35 72,39.5 81,40.5','B - - - m','soffit against the wall'),
('5','4','cap','T','J','v 48,15.5 59,17.5','B - - - m',''),
('4','6','cap','S','H','v 61,34.5 60,38 52,39.5 32,39','B - - - m','soffit against the wall'),
]
J['L']=[
('1','3','lean','S','G','v 96,29 92,26.5 86,28 78,31 74,35 70,50 68.5,58.5 | w 66,73','F - - - h',''),
('2','3','lean','J','G','v 58,40 54,60 | w 53,72','F - - - m',''),
('2','4b','lean','J','G','v 58,40 56,38 50,39.5 | w 47,43 44.5,58 | f 44,60 47,71','F g f n m',''),
('3','4b','lean','T','J','v 64,10.5 63,13 58,40','B - - - h','the main crack: skyline to the top of the low slab in front'),
('4a','4b','lean','T','J','w 47,12 46.5,25 | f 47,38 47,43','B g m y l','a crease'),
('4a','5','lean','T','H','v 33,14 32,20 30,36 28.5,46','B - - - m',''),
]
ADDED+=[('L4a','L4','left of the crease','the fourth leaner is two'),('L4b','L4','right of the crease','')]
J['M']=[
('1','2','col','J','G','v 72,54 68,57 65,64 63,75','F - - - h',''),
('1','4','other','J','S','v 72,54 85,53 91,55','F - - - h',''),
('2','3','col','J','G','v 54,44.5 53,45 51,55 50,70','F - - - m',''),
('2','4','col','J','J','v 70,42 74,44.5 73,52 72,54','F - - - m',''),
('2','5','other','J','J','v 56,43 57,42.5 70,42','F - - - m',''),
('3','5','col','S','J','v 51,32 54,34 54,41 54,44.5','B - - - m',''),
('4','5','col','J','J','v 73,29 70,31.5 69,36 70,42','B - - - m',''),
('4','6','other','J','S','v 73,29 84,27.5 86,28.5','B - - - m',''),
('5','6','col','S','J','v 67,17 72,19.5 72,29','B - - - m',''),
]
J['N']=[
('1','2','other','J','G','v 74,58 82,57 86,60 | f 88,66 88,80','S s f n m',''),
('1','3','other','J','G','v 76,58 72,58.5 58,63 55,66 51,69 49,74','F - - - h',''),
('2','3','other','J','J','v 85,41 83,41.5 79,47 75,57.5','F - - - m',''),
('2','5','other','J','S','v 83,41.5 90,39 94,41','B - - - m',''),
('3','4','lean','S','J','v 55,33.5 62,31.5 70,30','B - - - m',''),
('3','5','lean','J','J','v 77,29 80,28.5 85,30 86,36 84,41','B - - - m',''),
('4','5','lean','S','J','v 67,22.5 75,22.5 74,26.5 77,29','B - - - m',''),
]
J['O']=[
('1','2','other','J','G','v 77,62.5 73,63 69,66.5 66,71 64,77','F - - - h',''),
('1','3','other','J','S','v 78,62.5 86,61.5 91,64 | w 94,72','F - - - h',''),
('2','3','col','J','J','v 74,43.5 78,46 78,58 76.5,61','F - - - m',''),
('4','2','cap','J','J','v 53,40.5 64,43 74,43.5','F - - - h',''),
('2','7','col','J','G','v 50,40 47,43 45,55 | w 44.5,62 | f 44,69','F g f n l','the block against the set-back wall'),
('3','4','col','S','J','v 91,23 80,24 76,27 75,35 75,44','B - - - h',''),
('4','5','cap','J','S','v 58,17.5 55,20 54,25 52,36 52.5,38','B - - - h','the joint between the two upper blocks'),
('6','4','cap','J','S','v 58,17.5 70,16 78,15','B - - - m',''),
('6','5','cap','S','J','v 46,12.5 55,14 60,16.5 58,17.5','B - - - m',''),
('5','7','cap','S','J','v 37,38.5 50,40','B - - - m','soffit against the wall'),
]
J['P']=[
('1','2','col','J','G','w 78,46.5 78.5,51.5 | v 79,60 79.5,69 | w 79.5,74 | f 80,80.5','F g f n m',''),
('1','3','other','J','G','v 73,47.5 64,49.5 60,53 54,64 51,73','F - - - h',''),
('2','6','other','J','S','v 78,46.5 81,43 85,40.5 90,39.5 93,43','F - - - h',''),
('3','4','lean','T','H','v 64,18 63,18.5 60,22 50,42 40,66','B - - - m',''),
('3','6','lean','T','J','v 71,16 72,15.5 80,14 83,16 82,24 76,40 72,47.5','B - - - h',''),
('4','5','lean','T','H','v 38,26 32,42 32.5,46','B - - - m',''),
]
