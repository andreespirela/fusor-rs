await new Promise(resolve=>{
 if(document.documentElement.dataset.benchReady){resolve();return;}
 const observer=new MutationObserver(()=>{if(document.documentElement.dataset.benchReady){observer.disconnect();resolve();}});
 observer.observe(document.documentElement,{attributes:true,attributeFilter:['data-bench-ready']});
});
const boot=document.querySelector('script[src*="__fusor/"][src$="boot.js"]');
const unit=await import(new URL('pkg/app.js',boot.src));
const wasm=await unit.default();
const api={flush(){},wasmBytes(){return wasm.memory.buffer.byteLength;},hydrate(n){return unit.bench_hydrate(n);}};
for(const name of ['mount','unmount','update','bulk','insert','remove','swap','fanout','fanin'])api[name]=(...args)=>unit['bench_'+name](...args);
globalThis.__bench=api;
const query=new URLSearchParams(location.search);
if(query.has('startup'))api.mount(Number(query.get('startup')),'rows');
globalThis.__benchReady=performance.now();
