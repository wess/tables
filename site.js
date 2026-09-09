const headings=[...document.querySelectorAll('.docmain article[id]')];
const toc=document.querySelector('#toc');
if(toc){headings.forEach(section=>{const link=document.createElement('a');link.href=`#${section.id}`;link.textContent=section.querySelector('h2').textContent;toc.append(link)})}
const search=document.querySelector('#docsearch');
if(search){search.addEventListener('input',()=>{const term=search.value.trim().toLowerCase();headings.forEach(section=>{section.hidden=Boolean(term)&&!`${section.textContent} ${section.dataset.search||''}`.toLowerCase().includes(term)})})}
document.querySelectorAll('.copy').forEach(button=>button.addEventListener('click',async()=>{const code=button.parentElement.querySelector('code').textContent;await navigator.clipboard.writeText(code);button.textContent='Copied';setTimeout(()=>button.textContent='Copy',1200)}));
if(headings.length){const links=[...document.querySelectorAll('.docside a,.doctoc a')];const observer=new IntersectionObserver(entries=>entries.forEach(entry=>{if(entry.isIntersecting){links.forEach(link=>link.classList.toggle('active',link.hash===`#${entry.target.id}`))}}),{rootMargin:'-20% 0px -70%'});headings.forEach(section=>observer.observe(section))}
