// Run against a --headless --demo instance. Chromium runs completely off-screen.
const { chromium } = require('playwright');
const fs=require('node:fs');
const assert=require('node:assert/strict');
(async()=>{
  const port=process.env.ROUNDKEYS_PORT||17843;
  const browser=await chromium.launch({headless:true,executablePath:process.env.BROWSER_EXE||'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe'});
  try {
    const page=await browser.newPage({viewport:{width:652,height:524},deviceScaleFactor:1});
    const errors=[];page.on('pageerror',e=>errors.push(String(e)));
    await page.goto(`http://127.0.0.1:${port}/overlay`);
    await page.waitForFunction(()=>typeof state!=='undefined'&&state&&state.strokes.length>=4);
    const info=await page.evaluate(()=>({lanes:layout(state.config).lanes.length,dim:layout(state.config),transparent:state.config.background[3],strokes:state.strokes.length,connected}));
    assert.equal(info.lanes,info.dim.lanes.length);
    const backend=await(await page.request.get(`http://127.0.0.1:${port}/state`)).json();
    assert.equal(info.lanes,backend.config.key_count+backend.config.scratch_sides.length);
    assert.equal(backend.pressed.length,info.lanes);assert.equal(backend.counts.length,info.lanes);assert.equal(info.transparent,0);assert.equal(info.connected,true);
    const ordered=[...info.dim.lanes].sort((a,b)=>a.x-b.x);
    for(let i=1;i<ordered.length;i++)assert.ok(ordered[i-1].x+ordered[i-1].w<ordered[i].x);
    assert.ok(ordered.at(-1).x+ordered.at(-1).w<=info.dim.width-20);
    await page.setViewportSize({width:Math.ceil(info.dim.width),height:Math.ceil(info.dim.height)});
    // A held/released stroke must move up over time with unchanged timestamps.
    const oldY=await page.evaluate(()=>{const s=state.strokes.at(-3);window.__stroke={lane:s.lane,start:s.start};return layout(state.config).lanes[s.lane].y-(state.now-s.start)*state.config.speed/1000;});
    await page.waitForTimeout(150);
    const sample=await page.evaluate(()=>({now:state.now,pixels:Array.from(ctx.getImageData(0,0,1,1).data),count:ctx.getImageData(0,0,canvas.width,canvas.height).data.filter((v,i)=>i%4===3&&v>0).length}));
    const newY=await page.evaluate(()=>{const s=state.strokes.find(s=>s.lane===window.__stroke.lane&&s.start===window.__stroke.start);return layout(state.config).lanes[s.lane].y-(state.now-s.start)*state.config.speed/1000;});
    assert.ok(newY<oldY-20,'Released strokes must continue flowing upward');
    assert.equal(sample.pixels[3],0);assert.ok(sample.count>10000);
    fs.mkdirSync('tests/artifacts',{recursive:true});
    await page.screenshot({path:'tests/artifacts/overlay.png',omitBackground:true});
    const health=await(await page.request.get(`http://127.0.0.1:${port}/health`)).json();
    assert.ok(health.clients>=1);assert.match(health.input,/Raw Input|HID 监听/);
    // Reload behaves like OBS hide/show or browser source recreation.
    await page.reload();await page.waitForFunction(()=>state&&connected);
    assert.equal(errors.length,0,errors.join('\n'));
    console.log(JSON.stringify({result:'PASS',lanes:info.lanes,dimensions:[info.dim.width,info.dim.height],transparent:true,renderedPixels:sample.count,health},null,2));
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
