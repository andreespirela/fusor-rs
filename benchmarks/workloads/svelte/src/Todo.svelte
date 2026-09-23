<script>
 let tasks=$state(Array.from({length:20},(_,id)=>({id,title:`Task ${id+1}`,done:false})));
 let title=$state(''),filter=$state('all');
 let done=$derived(tasks.filter(task=>task.done).length);
 let visible=$derived(tasks.filter(task=>filter==='all'||task.done===(filter==='done')));
 function add(event){event.preventDefault();if(!title.trim())return;tasks.push({id:Date.now(),title:title.trim(),done:false});title='';}
</script>
<main><h1>Tasks</h1><form onsubmit={add}><input aria-label="New task" bind:value={title}><button>Add task</button></form><nav>{#each ['all','active','done'] as value}<button onclick={()=>filter=value}>{value}</button>{/each}</nav><p class="task-count">{tasks.length} tasks · {done} done</p><ul>{#each visible as task (task.id)}<li><label><input type="checkbox" bind:checked={task.done}><span>{task.title}</span></label><button aria-label="Delete task" onclick={()=>tasks=tasks.filter(item=>item.id!==task.id)}>×</button></li>{/each}</ul></main>
