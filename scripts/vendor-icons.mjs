import {execFileSync} from 'node:child_process';
import {mkdirSync, writeFileSync, readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const source = resolve(process.argv[2] || '');
const revision = execFileSync('git', ['-C', source, 'rev-parse', 'HEAD'], {encoding:'utf8'}).trim();
const {renderSVG} = await import(pathToFileURL(resolve(source, 'packages/core/src/render.js')));
const names = ['clipboard','star','monitor','list-ordered','settings','file-text','link','image','folder','code','palette','trash','search','checkbox','arrow-right','users','rotate-cw','download','copy','shield-check'];
const output = new URL('../assets/icons/', import.meta.url);
mkdirSync(output, {recursive:true});
const license = readFileSync(resolve(source, 'LICENSE-ICONS'), 'utf8');
const metadata = license.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');
for (const name of names) {
  const {default: definition} = await import(pathToFileURL(resolve(source, `icons/${name}.js`)));
  const svg = renderSVG(definition, {accent:'#3b8d74',size:28});
  writeFileSync(new URL(`${name}.svg`, output), svg.replace(/(<svg\b[^>]*>)/, `$1<metadata>${metadata}</metadata>`) + '\n');
}
writeFileSync(new URL('LICENSE', output), readFileSync(resolve(source, 'LICENSE-ICONS')));
writeFileSync(new URL('SOURCE.json', output), JSON.stringify({repository:'https://github.com/kubai087/kubai-icons', revision, variant:'color', accent:'#3b8d74', icons:names}, null, 2) + '\n');
