const d = document.body.dataset;
const current = Number(d.study);
document.querySelector('.study-nav').innerHTML = `<strong>Theme studies</strong><span>${d.name}</span>${[1,2,3,4,5].map(n => `<a href="theme-${n}.html" ${n === current ? 'aria-current="page"' : ''}>${n}</a>`).join('')}`;
document.querySelector('#study').innerHTML = `
  <section class="site-shell">
    <header class="site-head"><div class="brand"><i class="lotus"></i>dharmatiles</div><nav class="site-links"><a class="active">Overview</a><a>Language</a><a>Elements</a><a>Preview</a><a>Roadmap</a></nav></header>
    <div class="hero">
      <div><div class="eyebrow">Procedural · printable · yours</div><h1>Write a few lines. Print the terrain.</h1><p class="lede">Build watertight tabletop terrain from a small, readable document. See every change, trace every part, and know it will print before it reaches the slicer.</p><div class="actions"><button class="btn primary">Get dharmatiles</button><button class="btn">Read the language</button></div></div>
      <div class="plate"><img src="../../elements/img/el-meadow.jpg" alt="A rendered grassy terrain tile"><div class="code-chip"><b>tile</b> meadow size=<span class="str">"1x1"</span> {<br>&nbsp; stack { dirt; grass; scatter { rock } }<br>}</div></div>
    </div>
  </section>
  <section class="theme-meta"><div><div class="eyebrow">Direction ${current}</div><h2 id="study-name">${d.name}</h2><p id="study-note">${d.note}</p><p><strong id="direction">${d.direction}</strong></p></div><div class="swatches"><div class="swatch" style="--c:var(--bg)">ground</div><div class="swatch" style="--c:var(--panel)">panel</div><div class="swatch" style="--c:var(--raised)">raised</div><div class="swatch" style="--c:var(--accent);--swatch-label:var(--on-accent)">action</div><div class="swatch" style="--c:var(--accent2)">secondary</div></div></section>
  <section class="editor-section">
    <div class="editor-intro"><div><div class="eyebrow">The same visual language</div><h2>The editor</h2></div><p>The documentation becomes the quiet reference surface; the editor becomes a dense instrument. The tokens, typography and interaction color stay recognizably the same.</p></div>
    <div class="app">
      <header class="app-top"><div class="brand"><i class="lotus"></i>dharmatiles</div><span class="tile-name">meadow-rocks</span><span class="dim">1×1 · DungeonBlocks</span><span class="gate">passes</span></header>
      <section class="code-pane"><div class="pane-title"><span>meadow-rocks.dtile</span><span>saved</span></div><pre><span class="line"><span class="kw">tile</span> meadow-rocks size=<span class="str">"1x1"</span> {</span><span class="line">  <span class="cm">about "A rocky meadow."</span></span><span class="line">  stack {</span><span class="line">    dirt</span><span class="line">    grass cover=<span class="str">"68%"</span></span><span class="line sel">    scatter count=3 { rock size=<span class="str">"7..11"</span> }</span><span class="line">  }</span><span class="line">}</span></pre></section>
      <main class="view"><img src="../../elements/img/el-meadow.jpg" alt="The meadow tile in the editor"><i class="selection"></i><div class="view-tabs"><span class="on">Iso</span><span>Map</span><span>Low</span></div><div class="place">Place or draw: rock, tree, scatter, dirt, line, region…</div></main>
      <aside class="stack"><h3>Stack <small>in build order</small></h3><div class="stack-row" style="--family:var(--earth)"><div><b>dirt</b><small>surface · line 4</small></div></div><div class="stack-row" style="--family:var(--plant)"><div><b>grass</b><small>plant · line 5</small></div></div><div class="stack-row sel" style="--family:var(--accent)"><div><b>rocks</b><small>3 pieces · line 6</small></div></div><div class="stack-row" style="--family:var(--muted)"><div><b>DungeonBlocks base</b><small>added to every tile</small></div></div><div class="audit-dots"><span><i class="band"></i>55–65°</span><span><i class="over"></i>past 65°</span></div></aside>
    </div>
  </section>
  <footer class="study-foot">Theme study only · red and orange remain reserved for the printability gate</footer>`;
