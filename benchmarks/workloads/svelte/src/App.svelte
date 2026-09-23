<script>
  import Row from './Row.svelte';
  let { n, mode } = $props();
  let rows = $state(Array.from({length:n},(_,id)=>({id,value:id})));
  let shared = $state(0);
  let total = $derived(mode==='fanin'?rows.reduce((sum,item)=>sum+item.value,0):0);
  export function update(index,value){rows[index].value=value;}
  export function bulk(count){for(let i=0;i<count;i++)rows[i].value++;}
  export function insert(index,id){rows.splice(index,0,{id,value:id});}
  export function remove(index){rows.splice(index,1);}
  export function swap(a,b){[rows[a],rows[b]]=[rows[b],rows[a]];}
  export function fanout(value){shared=value;}
  export function fanin(){for(const row of rows)row.value++;}
</script>
<section><output id="total">{total}</output><ul>{#if mode!=='fanin'}{#each rows as item (item.id)}<Row {item} {mode} {shared}/>{/each}{/if}</ul></section>
