# Handle Pull Request Review

if $ARGUMENTS is empty - then get copilot review comments from current active git Pull Request
else if $ARGUMENTS is not empty - it might contain something like "PR NN" for a specific PR or just a number like "NN" - then just find that specific one and get the copilot review comments
else if the $ARGUMENTS is something completely different - figure out what it means and handle it creatively

**Steps**

1: copilot has made some review comments and now you have to figure out wether this is something we should solve or not. be critical of the review comments - validate/verify them using canonical design sources/documents and existing implementations.

1.5: all issues that you deem important can continue to step 2.

2: find good solutions to the problems

3: implement the solutions

4: give me a report of what was changed. also tell me about the things that were not and why. also tell me about how good the solution was and wether it was easy or hard and how much it affects other things.