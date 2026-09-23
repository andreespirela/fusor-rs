<script setup>
import { ref,computed } from 'vue';
import Row from './Row.vue';
const props=defineProps(['n','mode']);
const rows=ref(Array.from({length:props.n},(_,id)=>({id,value:id})));
const shared=ref(0);
const total=computed(()=>props.mode==='fanin'?rows.value.reduce((sum,item)=>sum+item.value,0):0);
defineExpose({
 update(index,value){rows.value[index].value=value;},bulk(count){for(let i=0;i<count;i++)rows.value[i].value++;},
 insert(index,id){rows.value.splice(index,0,{id,value:id});},remove(index){rows.value.splice(index,1);},
 swap(a,b){[rows.value[a],rows.value[b]]=[rows.value[b],rows.value[a]];},
 fanout(value){shared.value=value;},fanin(){for(const row of rows.value)row.value++;},
});
</script>
<template><section><output id="total">{{total}}</output><ul><template v-if="mode!=='fanin'"><Row v-for="item in rows" :key="item.id" :item="item" :mode="mode" :shared="shared"/></template></ul></section></template>
