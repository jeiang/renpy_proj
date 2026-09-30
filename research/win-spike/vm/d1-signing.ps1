# Q3 code-signing note: self-signed Authenticode on the single exe; run it after signing; check appended data behaviour.
param([string]$exe = 'C:\spike\target\c\release\win-host.exe')
$w = 'C:\spike\out\sign'; New-Item -ItemType Directory -Force $w | Out-Null
$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject 'CN=spike-selfsigned' -CertStoreLocation Cert:\LocalMachine\My
# trust it locally so the status can read Valid (a real cert chains to a public root)
Export-Certificate -Cert $cert -FilePath $w\c.cer | Out-Null
Import-Certificate -FilePath $w\c.cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
Import-Certificate -FilePath $w\c.cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
Copy-Item $exe $w\signed.exe -Force
Set-AuthenticodeSignature -FilePath $w\signed.exe -Certificate $cert -HashAlgorithm SHA256 | Out-Null
"1. sign after link:        " + (Get-AuthenticodeSignature $w\signed.exe).Status
$env:PROBE_EXEC = 'C:\spike\empty.py'
& $w\signed.exe | Out-Null; "   signed exe runs, exit=  $LASTEXITCODE"
"   size unsigned/signed:   {0} / {1}" -f (Get-Item $exe).Length, (Get-Item $w\signed.exe).Length
# overlay behaviour: append 1 MB after signing (breaks) vs before signing (fine)
Copy-Item $w\signed.exe $w\append_after.exe -Force
[IO.File]::AppendAllText("$w\append_after.exe", ('x' * 1048576))
"2. append after signing:   " + (Get-AuthenticodeSignature $w\append_after.exe).Status
Copy-Item $exe $w\append_before.exe -Force
[IO.File]::AppendAllText("$w\append_before.exe", ('x' * 1048576))
Set-AuthenticodeSignature -FilePath $w\append_before.exe -Certificate $cert -HashAlgorithm SHA256 | Out-Null
"3. append then sign:       " + (Get-AuthenticodeSignature $w\append_before.exe).Status
& $w\append_before.exe | Out-Null; "   append_before runs, exit= $LASTEXITCODE"
"4. tampered byte inside .text after signing:"
Copy-Item $w\signed.exe $w\tamper.exe -Force
$b = [IO.File]::ReadAllBytes("$w\tamper.exe"); $b[4096+100] = $b[4096+100] -bxor 1; [IO.File]::WriteAllBytes("$w\tamper.exe", $b)
"   " + (Get-AuthenticodeSignature $w\tamper.exe).Status
