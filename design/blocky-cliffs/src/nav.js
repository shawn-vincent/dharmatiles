{
  const CH = [['c1', 'The four sheets'], ['c2', 'Silhouettes and mass'], ['c3', 'Anatomy of one stone'], ['c4', 'Catalogue of stones'], ['c5', 'Proportions'], ['c6', 'Faces, edges, corners'], ['c7', 'Neighbours'], ['c8', 'Repeated arrangements'], ['c9', 'Hierarchy'], ['c10', 'Randomness'], ['c11', 'The grammar'], ['c12', 'Sensitivity and the style’s edges'], ['c13', 'A generator'], ['c14', 'Rules, ranges, constraints']];
  const nav = $('#chapters'), stubs = $('#stubs');
  CH.forEach(([id, name], i) => { nav.append(h('li', {}, h('a', { href: '#' + id }, h('span', { text: String(i + 1) }), name))); if (!document.getElementById(id)) stubs.append(h('section', { class: 'ch', id }, h('header', {}, h('span', { class: 'n', text: 'Chapter ' + (i + 1) }), h('h2', { text: name }), h('p', { text: 'Being measured and drawn.' })))); });
}
