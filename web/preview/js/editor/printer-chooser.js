// @ts-check
// The printer chooser on this page (js/view/printer-chooser.js is the widget): its calls to the program and where
// it says what went wrong, shared by every place a printer is chosen: the title bar's target card and the plates panel.

import { failed } from '../problems.js';
import { html } from '../view/html.js';
import { PrinterChooser } from '../view/printer-chooser.js';
import { host, toast } from './app.js';
import { server } from './stores.js';
import { squareOf } from '../printers.js';

/** The chooser as a vnode for `{printers, current, onPick, needBed, disabled, skey}`. @param {any} props */
export const chooserFor = (props) => html`<${PrinterChooser} square=${squareOf(server.get().system)} ...${props} scan=${host.printersScan} importFrom=${host.printersImport} say=${failed} toast=${toast} />`;
