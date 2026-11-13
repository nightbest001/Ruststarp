#!/usr/bin/env python3
"""Assemble ui-web/dist/index.html from the sources in ui-web/ (run after editing)."""
import base64,json,os,sys
here=os.path.dirname(os.path.abspath(__file__))
def r(p,mode='r'):return open(os.path.join(here,p),mode,encoding=None if 'b' in mode else 'utf-8').read()
icon=base64.b64encode(r('../assets/Ruststrap-256.png','rb')).decode()
strings=r('strings.json')
html=f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ruststrap</title><style>{r('style.css')}</style></head>
<body><div id="root"></div>
<script>window.APP_ICON="data:image/png;base64,{icon}";window.STRINGS={strings};</script>
<script>{r('icons.js')}</script>
<script>{r('core.js')}</script>
<script>if(!window.__INIT)window.__INIT=window.__DEV_INIT||null;</script>
<script>{r('views.js')}</script>
</body></html>'''
os.makedirs(os.path.join(here,'dist'),exist_ok=True)
open(os.path.join(here,'dist','index.html'),'w',encoding='utf-8').write(html)
print('dist/index.html',len(html),'bytes')
