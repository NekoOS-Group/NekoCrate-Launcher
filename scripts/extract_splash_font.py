import io,json,zipfile,pathlib,hashlib,argparse
from PIL import Image
parser=argparse.ArgumentParser(description='Extract vanilla Minecraft 1.21.1 splash glyphs without resampling. Requires Pillow.')
parser.add_argument('minecraft', type=pathlib.Path, help='Game directory containing versions/1.21.1 and assets/')
args=parser.parse_args()
root=pathlib.Path(__file__).resolve().parents[1];base=args.minecraft;jar=base/'versions/1.21.1/1.21.1.jar'
glyphs={}
with zipfile.ZipFile(jar) as z:
 definition=json.loads(z.read('assets/minecraft/font/include/default.json'))
 for p in reversed(definition['providers']):
  name='assets/minecraft/textures/'+p['file'].split(':')[1];im=Image.open(io.BytesIO(z.read(name))).convert('RGBA');cw=im.width//len(p['chars'][0]);ch=im.height//len(p['chars']);scale=p.get('height',8)/ch
  for ry,row in enumerate(p['chars']):
   for rx,c in enumerate(row):
    if c=='\0':continue
    bitmap=im.crop((rx*cw,ry*ch,(rx+1)*cw,(ry+1)*ch));rows=[];right=0
    for y in range(ch):
     n=0
     for x in range(cw):
      if bitmap.getpixel((x,y))[3]:n|=1<<(cw-1-x);right=max(right,x+1)
     packed=(cw+3)//4*4;rows.append(f'{n<<(packed-cw):0{packed//4}x}')
    glyphs[c]=[int(right*scale+.5)+1,scale,7-p['ascent'],1,packed,ch,''.join(rows)]
glyphs[' ']=[4,1,0,1,8,8,'0'*16];glyphs['\u200c']=[0,1,0,1,8,8,'0'*16]
(root/'app/src/assets/fonts/minecraft-bitmap.json').write_text(json.dumps(glyphs,ensure_ascii=False,separators=(',',':'))+'\n')
h='666999116c467f10622db1527a06ddc0a6efad2a'
with zipfile.ZipFile(base/'assets/objects'/h[:2]/h) as z:
 samplechars=set('祸不妄至，福不徒来。——司马迁《史记·龟策列传》');sample={}
 for line in z.read('unifont_all_no_pua-15.1.05.hex').decode().splitlines():
  code,bits=line.split(':');c=chr(int(code,16))
  if c in samplechars:sample[c]=bits
 (root/'app/src/assets/fonts/unifont-sample.json').write_text(json.dumps(sample,ensure_ascii=False,separators=(',',':'))+'\n')
 (root/'app/src/assets/fonts/UNIFONT-LICENSE.txt').write_bytes(z.read('LICENSE.txt'))
(root/'app/src/assets/fonts/provenance.json').write_text(json.dumps({'version':'Minecraft Java Edition 1.21.1','client_sha1':hashlib.sha1(jar.read_bytes()).hexdigest(),'bitmap_sources':[p['file'] for p in definition['providers']],'unicode_source':'https://resources.download.minecraft.net/66/'+h,'unicode_sha1':h,'unicode_version':'GNU Unifont 15.1.05','render_reference':'SplashRenderer (official mapping: fjq), render lines 24–35; UnihexProvider.Glyph (fls$d); BitmapProvider (flm)','notes':'Pixel coverage extracted unchanged. Bitmap glyphs: Mojang/Microsoft, not covered by repository code license. Unifont sample: see UNIFONT-LICENSE.txt. Full Unifont is loaded from installed assets or the official asset server at runtime; not bundled.'},ensure_ascii=False,indent=2)+'\n')
print(len(glyphs),'glyphs', (root/'app/src/assets/fonts/minecraft-bitmap.json').stat().st_size,'bytes')
