/* The walls the page opens with: each is a state of the controls (a name and the values it sets), nothing else. Anything a preset does not set keeps DEFAULTS. */
var PRESETS={
 J2:{name:'J corner on a 2x2 tile',p:{rows:2,tiles:2,regularity:0.3,wing:[20,26],front:30,top:[46,58],depth:22,width:[6,12,0.8],height:[10,15,1.0],bond:[0.05,0.6],shift:[0.25,0.5],setback:[0,0.6],wander:0.25,faces:1}},
 J:{name:'J corner: an L-shaped footprint',p:{regularity:0.3,wing:[13,7],front:10,top:[46,58],depth:20,width:[6,12,0.8],height:[10,15,1.0],bond:[0.05,0.6],shift:[0.25,0.5],setback:[0,0.6],wander:0.25,faces:1}},
 C:{name:'C spire: a free-standing tower',p:{regularity:0.25,foot:[17,17],top:[44,56],depth:17,width:[5,8,0.5],height:[9,13,0.5],bond:[0.1,0.5],shift:[0.25,0.5],setback:[0.4,1.0],wander:0.35,faces:1}},
 P:{name:'P spire: tall, tapering columns',p:{regularity:0.2,foot:[13,13],top:[56,68],depth:13,width:[4,7,0.5],height:[10,14,0.5],bond:[0.1,0.5],shift:[0.25,0.5],setback:[0.5,1.1],wander:0.4,faces:1}},
 D:{name:'D cap course, a lip over a recess',p:{regularity:0.3,top:[44,56],depth:22,width:[6,13,0.8],height:[10,15,1.0],bond:[0.05,0.6],shift:[0.25,0.5],setback:[0,0.6],wander:0.25,lip:[2.5,4],lipAt:'top',faces:1}},
 K:{name:'K recess, a deep lip',p:{regularity:0.3,top:[44,56],depth:22,width:[6,12,0.8],height:[10,14,1.0],bond:[0.05,0.6],shift:[0.25,0.5],setback:[0.6,1.6],wander:0.25,lip:[3,5],lipAt:'top',faces:1}},
 Lf:{name:'forms L: cantilevered ledges',p:{regularity:0.3,top:[40,50],depth:22,width:[7,12,0.8],height:[11,15,0.5],bond:[0.05,0.5],shift:[0.25,0.5],setback:[0,0.6],wander:0.2,front:15,depth:18,lip:[8,12],lipAt:[1,3],faces:1}},
 L:{name:'L wall, slabs below, broken top',p:{regularity:0.3,top:[48,60],depth:22,width:[5,13,0.8],height:[10,16,1.0],bond:[0.05,0.6],shift:[0.25,0.5],setback:[0,0.6],wander:0.25,faces:1}},
 B:{name:'B ragged tall wall, scree',p:{regularity:0.25,top:[50,64],depth:22,width:[6,12,1],height:[10,16,1],bond:[0.1,0.5],shift:[0.25,0.5],setback:[0,1],wander:0.35,scree:40,faces:1}},
 E:{name:'E terrace, stepped crest',p:{regularity:0.5,top:[24,32],depth:26,width:[8,14,0.5],height:[6.5,11,0.5],bond:[0.6,0.9],shift:[0.3,0.5],setback:[5,7],wander:0.15,faces:1}},
 M:{name:'M columns, a skyline',p:{regularity:0.2,top:[34,62],depth:22,width:[10,14,0],height:[10,16,0.5],bond:[0,0.04],shift:[0.25,0.5],setback:[0,0.4],wander:0.4,faces:1}},
 G:{name:'G leaning courses',p:{regularity:0.35,top:[46,56],depth:26,width:[7,12,0.8],height:[8,12,0.5],bond:[0.2,0.6],shift:[0.25,0.5],setback:[2,4],wander:0.3,faces:1}},
 O:{name:'O chunky blocks',p:{regularity:0.5,top:[40,54],depth:22,width:[14,18,0],height:[12,16,0],bond:[0.4,0.6],shift:[0.25,0.5],setback:[0,1.5],wander:0.5,faces:1}},
 F:{name:'F sheer wall',p:{regularity:0.4,top:[46,56],depth:22,width:[10,14,0],height:[12,16,0],bond:[0.1,0.3],shift:[0.25,0.5],setback:[0,0],wander:0.25,faces:1}},
 A:{name:'A low massif',p:{regularity:0.3,top:[14,20],depth:22,width:[8,13,0.5],height:[6,9,0.5],bond:[0.3,0.7],shift:[0.25,0.5],setback:[0,1],wander:0.35,faces:1}},
 N:{name:'forms N: masonry wall (running bond)',p:{regularity:1,top:[24,24],depth:7,width:[11,11,0],height:[12,12,0],bond:[1,1],shift:[0.5,0.5],setback:[0,0],wander:0,faces:2,scree:0}}
};
/* two walls that are meant to be refused by the gate, so the refusal can be seen: the L wall with courses only 4 to 6 high, and with wear and chips turned up until grooves are roofed over. `refuses` marks them; the sweep asserts the gate does refuse them. */
PRESETS.fHeight={name:'L, courses 4 to 6 high: thin courses, red',refuses:true,p:Object.assign({},PRESETS.L.p,{height:[4,6,0]})};
PRESETS.fWear={name:'L, wear 0.9 mm and chips 14: grooves roofed over',refuses:true,p:PRESETS.L.p,rock:{wear:0.9,chips:14}};
/* what the row planes and the merges reach that the older presets did not (each preset is the dials, nothing else) */
PRESETS.T={name:'T tall stones beside one-row stones',p:Object.assign({},PRESETS.L.p,{rise:0.6,span:0.3,wander:0})};
PRESETS.R={name:'R tilted rows, irregular fieldstone',p:Object.assign({},PRESETS.L.p,{regularity:0.05,width:[5,10,0.8],height:[8,12,1.0],bond:[0.5,0.9],dip:2,wander:0})};
PRESETS.S={name:'S near-slab: few joints, tall and wide',p:Object.assign({},PRESETS.L.p,{rise:0.7,span:0.7,wander:0})};
/* the rock of each wall preset, tuned by eye and by the gate (17 Nov): the default (WALLROCK: cuts, a little grain, wear) reads best and passes on the plain walls; a spire, a lip or a tilted row loses its cuts, because a cut on a tapering or overhanging cell leaves the stone above a roof over a notch */
['C','S','J2','G','P','Lf','D','J','T','R'].forEach(function(k){if(PRESETS[k])PRESETS[k].rock={cuts:0,wear:0.1,grain:0.1}});
var LAST={L:'sL',B:'sB',E:'sE',M:'sM',G:'sG',O:'sO',F:'sF',A:'sA',N:'fN',J:'sJ',J2:'sJ',D:'sD',K:'sK',Lf:'fL',C:'sC',P:'sP',T:'sL',R:'sL',S:'sA'};
LAST.fHeight='sL';LAST.fWear='sL';
/* the order of the list, and the groups it is shown in */
var PRESET_ORDER=['L','B','E','M','G','O','F','A','N','D','K','Lf','J','J2','C','P','T','R','S','fHeight','fWear'];
