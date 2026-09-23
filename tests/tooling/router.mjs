import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { resolve, extname, sep } from 'node:path';
import { chromium, firefox, webkit, expect } from '@playwright/test';
import { buildPackage } from '../../scripts/build.mjs';
await buildPackage('fusor-router-example');
const root=resolve('examples/router/dist');
const server=createServer(async(req,res)=>{
 const url=new URL(req.url,'http://localhost');
 const name=url.pathname.slice('/nested/'.length);
 const file=resolve(root,extname(name)?name:'index.html');
 if(!file.startsWith(root+sep)){res.writeHead(404).end();return;}
 try{res.setHeader('content-type',({'.html':'text/html','.js':'text/javascript','.wasm':'application/wasm'})[extname(file)]||'text/plain');res.end(await readFile(file));}catch{res.writeHead(404).end();}
});
await new Promise(done=>server.listen(0,'127.0.0.1',done));
let browser,errors=[];
try {
 for(const name of (process.env.PLAYWRIGHT_BROWSERS||'chromium').split(',')){
  browser=await {chromium,firefox,webkit}[name].launch(name==='chromium'&&process.env.PLAYWRIGHT_CHANNEL?{channel:process.env.PLAYWRIGHT_CHANNEL}:{});
  const page=await browser.newPage();errors=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('console',e=>{if(e.type()==='error')errors.push(e.text());});
  const origin=`http://127.0.0.1:${server.address().port}/nested/`;
  await page.goto(origin+'dashboard/settings/preferences');
  await expect(page.locator('#settings')).toBeVisible();
  await page.evaluate(async()=>{const boot=document.querySelector('script[type="module"]').src;window.client=await import(new URL('./pkg/app.js',boot).href);});
  await page.evaluate(()=>window.client.navigate('/nested/memory',true));
  await expect(page.locator('#memory p')).toHaveText('first');
  const memoryUrl=page.url();
  const rejected=await page.evaluate(()=>{
   window.client.memory_stage('/second');
   let rejected=false;try{window.client.memory_stage('/third');}catch{rejected=true;}
   window.client.memory_finish(false);return rejected;
  });
  assert(rejected,'only one prepared transaction at a time');
  await expect(page.locator('#memory p')).toHaveText('first');
  await page.evaluate(()=>{window.client.memory_stage('/second');window.client.memory_finish(true);});
  await expect(page.locator('#memory p')).toHaveText('second');
  assert.equal(page.url(),memoryUrl,'memory navigation leaves browser history alone');
  assert.match(await page.evaluate(()=>{try{window.client.memory_stage('/fail');return '';}catch(e){return String(e);}}),/memory preparation failed/);
  await expect(page.locator('#memory p')).toHaveText('second');
  await page.evaluate(()=>window.client.navigate('/nested/dashboard/settings',true));
  assert.match(await page.evaluate(()=>{try{window.client.memory_stage('/second');return '';}catch(e){return String(e);}}),/unmounted/);
  await page.evaluate(()=>window.client.navigate('/nested/teams/one',true));
  await page.locator('#team-count').click();
  await page.evaluate(()=>window.client.navigate('/nested/teams/one/members/42',true));
  await expect(page.locator('#team-count')).toHaveText('1');
  await expect(page.locator('#member')).toHaveText('Member 42');
  await page.evaluate(()=>window.client.navigate('/nested/teams/two/members/7',true));
  await expect(page.locator('#team-count')).toHaveText('0');
  await expect(page.locator('#team h1')).toHaveText('Team two');
  await expect(page.locator('#member')).toHaveText('Member 7');
  await page.evaluate(()=>window.client.navigate('/nested/lexical/hello',true));
  await page.locator('#capture-button').click();
  await page.locator('#capture-button').click();
  await expect(page.locator('#selected')).toHaveText('hello');
  await expect(page.locator('li')).toHaveText(['hello 1','hello 2']);
  await page.evaluate(()=>window.client.navigate('/nested/dashboard/settings',true));
  const initialUrl=page.url();
  for(const replace of [false,true]) {
   const failure=await page.evaluate(replace=>{try{window.client.navigate('/nested/broken',replace);return '';}catch(error){return String(error);}},replace);
   assert.match(failure,/expected page setup failure/);
   assert.equal(page.url(),initialUrl);
   await expect(page.locator('#settings')).toBeVisible();
  }
  await page.evaluate(()=>window.client.navigate('/nested/dashboard/settings/preferences',true));
  await expect(page.locator('#preferences')).toBeVisible();
  await page.evaluate(()=>window.client.navigate('/nested/dashboard/settings',true));
  await expect(page.locator('router, route')).toHaveCount(0);
  await page.locator('#parent-count').click();
  await page.locator('#child-count').click();
  await page.locator('#dashboard').evaluate(el=>window.dashboard=el);
  await page.getByRole('link',{name:'Overview',exact:true}).click();
  await expect(page.locator('#overview')).toBeVisible();
  await expect(page.locator('#parent-count')).toHaveText('Parent 1');
  assert(await page.locator('#dashboard').evaluate(el=>el===window.dashboard));
  await page.goBack();
  await expect(page.locator('#child-count')).toHaveText('Child 0');
  await expect(page.locator('#parent-count')).toHaveText('Parent 1');
  await page.getByRole('link',{name:'Filter',exact:true}).click();
  await expect(page.locator('#filter')).toHaveText('one');
  await page.locator('#filter').evaluate(el=>window.filter=el);
  await page.getByRole('link',{name:'Filter two',exact:true}).click();
  await expect(page.locator('#filter')).toHaveText('two');
  assert(await page.locator('#filter').evaluate(el=>el===window.filter));
  await page.getByRole('link',{name:'Missing',exact:true}).click();
  await expect(page.locator('#nested-missing')).toBeVisible();
  await expect(page.locator('#parent-count')).toHaveText('Parent 1');
  await page.getByRole('link',{name:'Article',exact:true}).click();
  await expect(page.locator('h1')).toHaveText('Article hello');
  await expect(page.locator('#dashboard')).toHaveCount(0);
  await page.getByRole('link',{name:'Dashboard',exact:true}).click();
  await expect(page.locator('#parent-count')).toHaveText('Parent 0');
  await page.goto(origin+'articles/a%2Fb');
  await expect(page.locator('h1')).toHaveText('Article a/b');
  await page.goto(origin+'missing');
  await expect(page.locator('h1')).toHaveText('Page not found');
  await page.evaluate(async()=>{const boot=document.querySelector('script[type="module"]').src;window.client=await import(new URL('./pkg/app.js',boot).href);window.client.stop();});
  await expect(page.locator('main h1')).toHaveCount(0);
  assert.deepEqual(errors,['expected page setup failure','expected page setup failure']);
  console.log(`PASS ${name}: nested routers across files, deep links, preserved parent identity, query reactivity, scoped fallbacks, decoding and Back/Forward`);
  await browser.close();browser=null;
 }
} catch(error) {console.error('Browser errors:',errors);throw error;
} finally {await browser?.close();server.closeAllConnections();await new Promise(done=>server.close(done));}
