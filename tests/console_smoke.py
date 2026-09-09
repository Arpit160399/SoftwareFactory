#!/usr/bin/env python3
"""Real pseudo-terminal keyboard and lifecycle checks for the Rust control panel."""
import errno,fcntl,json,os,pathlib,pty,re,select,struct,subprocess,sys,tempfile,termios,time,unicodedata
binary=str(pathlib.Path(sys.argv[1]).resolve())
def render(data,width=110,height=32):
    grid=[[' ']*width for _ in range(height)];row=col=0
    for token in re.findall(r'\x1b\[[0-9;?]*[A-Za-z]|[^\x1b]',data.decode(errors='replace')):
        if token.startswith('\x1b['):
            code=token[-1];parts=token[2:-1].lstrip('?').split(';');nums=[int(x) if x else 0 for x in parts]
            if code in ('h','l') and nums[0]==1049:grid=[[' ']*width for _ in range(height)];row=col=0
            elif code in ('H','f'):row=max(0,(nums[0]or 1)-1);col=max(0,((nums[1]if len(nums)>1 else 1)or 1)-1)
            elif code=='G':col=max(0,(nums[0]or 1)-1)
            elif code=='J' and nums[0]in(2,3):grid=[[' ']*width for _ in range(height)]
            elif code=='K' and row<height:
                for c in range(min(col,width),width):grid[row][c]=' '
            elif code=='C':col+=nums[0]or 1
            elif code=='D':col=max(0,col-(nums[0]or 1))
            continue
        if token=='\r':col=0
        elif token=='\n':row+=1
        elif ord(token)>=32:
            if row<height and col<width:grid[row][col]=token
            col+=2 if unicodedata.east_asian_width(token)in('W','F')else 1
    return '\n'.join(''.join(line)for line in grid)
class Session:
    def __init__(self,root,width=110,height=32):
        self.width,self.height=width,height;self.master,self.slave=pty.openpty();self.before=termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave,termios.TIOCSWINSZ,struct.pack('HHHH',height,width,0,0));self.output=bytearray()
        self.process=subprocess.Popen([binary,'--project',str(root),'tui'],stdin=self.slave,stdout=self.slave,stderr=self.slave,env=dict(os.environ,TERM='xterm-256color'))
    def read(self):
        if select.select([self.master],[],[],.05)[0]:
            try:self.output.extend(os.read(self.master,65536))
            except OSError as e:
                if e.errno!=errno.EIO:raise
    def screen(self):return render(self.output,self.width,self.height)
    def wait(self,text,seconds=12):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            self.read()
            if text in self.screen():
                until=time.monotonic()+.18
                while time.monotonic()<until:self.read()
                if text in self.screen():return
            if self.process.poll()is not None:break
        raise AssertionError('Missing '+text+'\n'+self.screen())
    def key(self,key):os.write(self.master,key)
    def close(self):
        self.key(b'\x1b');until=time.monotonic()+.2
        while time.monotonic()<until:self.read()
        self.key(b'q');end=time.monotonic()+12
        while self.process.poll()is None and time.monotonic()<end:self.read()
        if self.process.poll()is None:self.process.kill();raise AssertionError('TUI did not close')
        assert self.process.returncode==0,self.screen();assert termios.tcgetattr(self.slave)==self.before,'terminal settings were not restored'
        os.close(self.master);os.close(self.slave)
with tempfile.TemporaryDirectory()as temp:
    root=pathlib.Path(temp);s=Session(root)
    try:
        s.wait('Project setup needed');s.key(b'3');s.wait('No tasks in this column');s.key(b'4');s.wait('No decisions are waiting');s.key(b'5');s.wait('No saved workflows')
        s.key(b'1');s.wait('Project home');s.key(b'n');s.wait('Start a whole workflow');s.key(b'\x1b');s.wait('Project home')
        s.key(b'e');s.wait('PROJECT SETUP');s.key(b'\r');s.wait('Review exact changes');s.key(b'\x1b');s.wait('Tab / Shift+Tab');time.sleep(.15);s.key(b'\x1b');s.wait('Project home')
        assert not(root/'.product-workflow/profile.json').exists()
    finally:s.close()
with tempfile.TemporaryDirectory()as temp:
    root=pathlib.Path(temp);(root/'.softwarefactory-synthetic-fixture').write_text('test only');(root/'AGENTS.md').write_text('Preserve guidance')
    repo=pathlib.Path(__file__).resolve().parents[1];bridge=str(repo/'examples/fixture_bridge.py')
    profile=json.loads(subprocess.check_output([binary,'template','generic','--id','terminal-test']))
    profile['product_brief']='Inspect saved product progress';profile['runtime']['command']={'program':bridge,'args':[]};profile['runtime']['planner_model']='fixture-planner';profile['review']['command']={'program':bridge,'args':[]};profile['review']['reviewers']=['fixture-human'];profile['checks']=[{'id':'result','category':'delivered_behaviour','required':True,'command':{'program':bridge,'args':[]},'criterion_ids':['C1'],'timeout_seconds':10}]
    draft=root/'draft.json';draft.write_text(json.dumps(profile));subprocess.run([binary,'--project',str(root),'setup','--profile',str(draft),'--apply'],check=True,capture_output=True)
    (root/'.fixture-propose').write_text('yes')
    s=Session(root)
    try:
        s.wait('terminal-test');s.key(b'p');s.wait('Capabilities verified');s.key(b'n');s.wait('Start a whole workflow');s.key(b'Improve onboarding\t1\r');s.wait('AwaitingBuildApproval',seconds=20)
        s.key(b'3');s.wait('Discovery');s.key(b'\x1b[C');s.wait('Needs approval');s.key(b'\r');s.wait('Acceptance criteria');s.key(b'\x1b');s.wait('←→ Columns');time.sleep(.15);s.key(b'/');s.wait('Filter tasks');s.key(b'no match\r');s.wait('No tasks in this column');s.key(b'c');s.wait('Synthetic visible');s.key(b'4');s.wait('Review inbox')
        snapshot=repo/'docs/console-terminal.txt';snapshot.write_text(s.screen())
        s.key(b' ');s.wait('Runner paused');s.key(b'6');s.wait('Project settings');s.key(b'v');s.wait('running_version');s.key(b'\x1b')
    finally:s.close()
    assert(root/'AGENTS.md').read_text()=='Preserve guidance'
    assert not(root/'result.txt').exists(),'unapproved implementation ran'
    s=Session(root,44,16)
    try:s.wait('SOFTWARE FACTORY');s.key(b'3');s.wait('Discovery');s.key(b'\x1b[C');s.wait('Needs approval')
    finally:s.close()
print('PASS: actual TUI navigation, setup return, prerequisite probes, start/approval pause, task details, search/clear, settings, narrow layout and terminal restoration')
