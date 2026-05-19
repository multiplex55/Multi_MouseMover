@echo off
REM Ensure this script is run from the root of a Git repository

echo Cleaning up local branches except 'main' and 'master'...
:: Filter out both main and master locally
FOR /F "tokens=*" %%B IN ('git branch ^| findstr /V "master" ^| findstr /V "main"') DO (
    echo Deleting local branch: %%B
    git branch -D %%B
)

echo.
echo Cleaning up remote branches except 'origin/main' and 'origin/master'...
git fetch --prune

:: Filter out both main and master from remote list
FOR /F "tokens=*" %%R IN ('git branch -r ^| findstr /V "origin/master" ^| findstr /V "origin/main"') DO (
    SETLOCAL ENABLEDELAYEDEXPANSION
    SET "BRANCH=%%R"
    SET "BRANCH=!BRANCH:origin/=!"
    :: Trim whitespace
    FOR /f "tokens=*" %%G IN ("!BRANCH!") DO SET "BRANCH=%%G"
    
    echo Deleting remote branch: !BRANCH!
    git push origin --delete !BRANCH!
    ENDLOCAL
)

echo.
echo Branch cleanup complete.
pause
