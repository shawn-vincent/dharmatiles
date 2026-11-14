// The page's view layer is Preact (vendored under vendor/, no build step) written with htm's tagged templates:
//
//   html`<button class="btn" onClick=${go}>Go</button>`
//
// A component is a function of the model (js/editor/model.js): it reads signals while it draws and is drawn again
// when one of them changes.  It keeps no copy of a fact the model holds, and the one thing it may keep itself is
// what the user is in the middle of (see field.js).  This module is the one place the three vendored files meet.

import { h, render, Fragment } from 'preact';
import { useEffect, useLayoutEffect, useRef, useState } from 'preact/hooks';
import htm from 'htm';
import '@preact/signals'; // installs the hook that draws a component again when a signal it read changes

export const html = htm.bind(h);
export { h, render, Fragment, useEffect, useLayoutEffect, useRef, useState };
