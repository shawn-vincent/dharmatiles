"""Crops of the reference sheets and of our comparison renders, written as small JPEGs into img/.
Sources are read only. Run by build.py."""
import os
from PIL import Image
HERE=os.path.dirname(os.path.abspath(__file__))
# The repo's reference/cliffs/ and the folder holding the wall renders (cliff-o/, cliff-slope/); both set from outside.
REPO=os.environ.get('DT_REFS', os.path.join(HERE,'..','..','reference','cliffs'))+'/'
SCR=os.environ.get('DT_RENDERS', '')+'/'
FL=REPO+'flat-rock-surface.jpg'; TX=REPO+'rock-surface-texture.jpg'; NO=REPO+'n-o-sculpting-sheet.jpg'
NS=REPO+'n-o-sculpting-study.jpg'; AP=REPO+'a-p-family-guide.jpg'; ON=REPO+'on-squares-a-p-unpainted.jpg'
# name: (source, (x0,y0,x1,y1), max width)
C={
 'f_dish':(FL,(1218,88,1535,152)),'f_ridge':(FL,(1218,152,1535,222)),'f_facet':(FL,(1218,222,1535,290)),
 'f_chip':(FL,(1218,290,1535,358)),'f_pit':(FL,(1218,358,1535,425)),'f_crack':(FL,(1218,425,1535,495)),
 'f_close':(FL,(398,88,1196,498),900),'f_profile':(FL,(12,545,465,740)),'f_table':(FL,(478,545,858,725)),
 'f_multi':(FL,(868,545,1222,750)),'f_patterns':(FL,(1235,540,1535,745)),'f_gallery':(FL,(12,795,772,1015),900),
 'f_rules':(FL,(780,790,1138,1015)),'f_workflow':(FL,(1145,790,1535,1015)),
 't_a':(TX,(5,82,262,168)),'t_b':(TX,(5,168,262,252)),'t_c':(TX,(5,252,262,330)),'t_d':(TX,(5,330,262,405)),'t_e':(TX,(5,405,262,472)),'t_f':(TX,(5,472,262,552)),
 't_big':(TX,(268,55,822,558),800),
 't_facet':(TX,(835,85,1195,170)),'t_edge':(TX,(835,172,1195,245)),'t_step':(TX,(835,245,1195,322)),'t_dish':(TX,(835,320,1195,395)),
 't_chip':(TX,(835,395,1195,470)),'t_ridge':(TX,(835,470,1195,548)),
 't_xsec':(TX,(1210,85,1535,335)),'t_profile':(TX,(1210,350,1535,560)),
 't_forms':(TX,(8,590,1035,765),1000),'t_scales':(TX,(1050,595,1535,760)),'t_pattern':(TX,(5,800,500,1015)),'t_lod':(TX,(505,800,985,1015)),
 'n_dims':(NO,(465,55,765,250)),'n_mass':(NO,(465,260,765,405)),'o_dims':(NO,(1228,55,1530,250)),'o_mass':(NO,(1228,260,1530,405)),
 'n_multi':(NO,(12,415,530,580)),'o_multi':(NO,(785,415,1300,580)),'n_geom':(NO,(12,585,530,750)),'o_geom':(NO,(785,585,1300,750)),
 'o_angles':(NO,(1315,585,1530,750)),'n_angles':(NO,(540,585,760,750)),'no_anat':(NO,(505,935,860,1015)),'no_lib':(NO,(12,935,500,1015)),
 'ap_angles':(AP,(452,470,965,695),800),'ap_hier':(AP,(975,470,1440,695)),'ap_prims':(AP,(980,85,1440,275)),'ap_tex':(AP,(5,730,440,920)),
 'ap_mist':(AP,(1115,945,1440,1075)),'ap_recipe':(AP,(5,945,640,1075),800),
 'ns_fracO':(NS,(730,350,1085,515)),'ns_planeO':(NS,(1095,350,1440,515)),'ns_carveO':(NS,(730,520,1440,660),900),'ns_carveN':(NS,(5,520,722,660),900),
 'ns_fracN':(NS,(5,350,350,515)),'ns_planeN':(NS,(360,350,722,515)),
 'ns_motifO':(NS,(730,668,1145,785)),'ns_motifN':(NS,(5,668,415,785)),'ns_mist':(NS,(1110,925,1440,1075)),
 'ref_o':(ON,(628,941,938,1251),620),'ref_n':(ON,(316,942,624,1250),616),
}
# our own renders: the right half (the model) of the side-by-side comparisons
CO=SCR+'cliff-o/'; CS=SCR+'cliff-slope/'
R={
 'm_o_boxes':(CO+'comp3.png',(582,0,1164,582)),'m_o_slits':(CO+'comp9.png',(582,0,1164,582)),'m_o_foil':(CO+'comp17.png',(582,0,1164,582)),
 'm_o_rubble':(CO+'comp35.png',(582,0,1164,582)),'m_o_rubble2':(CO+'comp33.png',(582,0,1164,582)),'m_o_last':(CO+'comp70.png',(0,0,1164,582)),
 'm_n_shards':(CS+'comp3.png',(582,0,1164,582)),'m_n_shards2':(CS+'comp7.png',(582,0,1164,582)),'m_n_black':(CS+'comp9.png',(582,0,1164,582)),
 'm_n_plates':(CS+'comp27.png',(582,0,1164,582)),'m_n_stripes':(CS+'comp17.png',(582,0,1164,582)),'m_n_last':(CS+'c_h.png',(0,0,1164,582)),
 'm_o_unlit':(CO+'comp1.png',(582,0,1164,582)),
}
def run():
    os.makedirs(HERE+'/img',exist_ok=True); cache={}
    def src(p):
        if p not in cache: cache[p]=Image.open(p).convert('RGB')
        return cache[p]
    for name,v in list(C.items())+list(R.items()):
        p,box=v[0],v[1]; mw=v[2] if len(v)>2 else (420 if name.startswith('m_') else 700)
        if name.startswith('m_') and v is R.get(name) and name.endswith('last'): mw=900
        im=src(p).crop(box)
        if im.width>mw: im=im.resize((mw,round(im.height*mw/im.width)),Image.LANCZOS)
        im.save(HERE+'/img/%s.jpg'%name,quality=80,optimize=True)
    # the eight characteristic surface forms, one by one
    xs=[10,148,280,410,540,668,797,922]; tx=src(TX)
    for i,x in enumerate(xs):
        im=tx.crop((x,592,x+126,765)); im=im.resize((int(im.width*1.6),int(im.height*1.6)),Image.LANCZOS); im.save(HERE+'/img/form%d.jpg'%i,quality=82)
    # patterns of G (flat sheet), four panels
    fl=src(FL)
    for i,b in enumerate([(1238,563,1380,637),(1383,563,1530,637),(1238,643,1380,735),(1383,643,1530,735)]):
        im=fl.crop(b); im=im.resize((int(im.width*2),int(im.height*2)),Image.LANCZOS); im.save(HERE+'/img/pat%d.jpg'%i,quality=82)
if __name__=='__main__': run()
