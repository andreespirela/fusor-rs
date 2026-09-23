<script>
  let { item, mode, shared } = $props();
  let computed = $derived(item.value * 2);
</script>
<li data-id={item.id}><span class="value">{mode==='computed'?computed:mode==='fanout'?item.value+shared:item.value}</span><button onclick={()=>item.value++}>+</button></li>
