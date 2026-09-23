<script setup>
import {ref,computed} from 'vue';
const tasks=ref(Array.from({length:20},(_,id)=>({id,title:`Task ${id+1}`,done:false})));
const title=ref(''),filter=ref('all');
const done=computed(()=>tasks.value.filter(task=>task.done).length);
const visible=computed(()=>tasks.value.filter(task=>filter.value==='all'||task.done===(filter.value==='done')));
function add(){if(!title.value.trim())return;tasks.value.push({id:Date.now(),title:title.value.trim(),done:false});title.value='';}
</script>
<template><main><h1>Tasks</h1><form @submit.prevent="add"><input aria-label="New task" v-model="title"><button>Add task</button></form><nav><button v-for="value in ['all','active','done']" :key="value" @click="filter=value">{{value}}</button></nav><p class="task-count">{{tasks.length}} tasks · {{done}} done</p><ul><li v-for="task in visible" :key="task.id"><label><input type="checkbox" v-model="task.done"><span>{{task.title}}</span></label><button aria-label="Delete task" @click="tasks=tasks.filter(item=>item.id!==task.id)">×</button></li></ul></main></template>
