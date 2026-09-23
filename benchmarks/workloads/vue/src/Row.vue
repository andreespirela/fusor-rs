<script setup>
import { computed } from 'vue';
const props=defineProps(['item','mode','shared']);
const derived=computed(()=>props.item.value*2);
</script>
<template><li :data-id="item.id"><span class="value">{{mode==='computed'?derived:mode==='fanout'?item.value+shared:item.value}}</span><button @click="item.value++">+</button></li></template>
