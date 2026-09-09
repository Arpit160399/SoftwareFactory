#!/usr/bin/env python3
"""Drive actual terminal events: preview, cancel, apply, repeat and detach."""
import errno,fcntl,json,os,pathlib,pty,select,struct,subprocess,sys,tempfile,termios,time,re,unicodedata
binary=str(pathlib.Path(sys.argv[1]).resolve())
def rendered(data):
    grid=[[' ']*110 for _ in range(32)];row=col=0
    for token in re.findall(r'\x1b\[[0-9;?]*[A-Za-z]|[^\x1b]',data.decode(errors='replace')):
        if token.startswith('\x1b['):
            code=token[-1];parts=token[2:-1].lstrip('?').split(';');nums=[int(x) if x else 0 for x in parts]
            if code in ('H','f'):row=max(0,(nums[0] or 1)-1);col=max(0,((nums[1] if len(nums)>1 else 1)or 1)-1)
            elif code=='G':col=max(0,(nums[0]or 1)-1)
            elif code=='J' and nums[0] in (2,3):grid=[[' ']*110 for _ in range(32)]
            elif code=='K' and row<32:
                for c in range(min(col,110),110):grid[row][c]=' '
            elif code=='C':col+=nums[0]or 1
            elif code=='D':col=max(0,col-(nums[0]or 1))
            continue
        if token=='\r':col=0
        elif token=='\n':row+=1
        elif ord(token)>=32:
            if row<32 and col<110:grid[row][col]=token
            col+=2 if unicodedata.east_asian_width(token) in ('W','F') else 1
    return '\n'.join(''.join(line) for line in grid)
def session(root,apply=False):
    master,slave=pty.openpty();fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',32,110,0,0))
    before=termios.tcgetattr(slave)
    process=subprocess.Popen([binary,'--project',str(root),'tui','--setup'],stdin=slave,stdout=slave,stderr=slave,env=dict(os.environ,TERM='xterm-256color'))
    output=bytearray()
    def wait_for(text):
        deadline=time.monotonic()+10
        while time.monotonic()<deadline:
            if text in rendered(output):return
            if select.select([master],[],[],.1)[0]:
                try:output.extend(os.read(master,65536))
                except OSError as e:
                    if e.errno==errno.EIO:break
                    raise
        raise AssertionError(f'Missing TUI text {text}: '+output.decode(errors='replace')[-2000:])
    try:
        wait_for('SOFTWARE FACTORY');os.write(master,b'\r');wait_for('Review exact changes')
        assert not (root/'.product-workflow/profile.json').exists() or apply=='repeat'
        if apply:
            os.write(master,b'y');wait_for('Configuration saved');os.write(master,b'\r')
        else:
            os.write(master,b'\x1b');time.sleep(.15);os.write(master,b'\x1b')
        deadline=time.monotonic()+10
        while process.poll() is None and time.monotonic()<deadline:
            if select.select([master],[],[],.1)[0]:
                try:output.extend(os.read(master,65536))
                except OSError as e:
                    if e.errno!=errno.EIO:raise
        if process.poll() is None:raise AssertionError('TUI did not exit: '+rendered(output))
        assert process.returncode==0
        assert termios.tcgetattr(slave)==before,'Terminal settings were not restored'
    finally:
        if process.poll() is None:process.kill();process.wait()
        os.close(master);os.close(slave)
with tempfile.TemporaryDirectory(prefix='softwarefactory-tui-') as folder:
    root=pathlib.Path(folder);(root/'AGENTS.md').write_text('Keep existing guidance.\n')
    session(root);assert not (root/'.product-workflow').exists(),'Cancellation wrote setup'
    session(root,True);profile=json.loads((root/'.product-workflow/profile.json').read_text());assert profile['runtime']['command']['program']==''
    assert not (root/'.product-workflow/runs').exists(),'Setup dispatched a feature'
    session(root,'repeat')
    assert (root/'AGENTS.md').read_text()=='Keep existing guidance.\n'
    subprocess.run([binary,'--project',str(root),'detach','--apply'],check=True,stdout=subprocess.DEVNULL)
    assert not (root/'.product-workflow/profile.json').exists()
print('PASS: actual TUI preview, cancel, apply, repeat, terminal restoration, guidance preservation and safe detach')
