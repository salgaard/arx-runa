# Handle Pull Request Review

if $ARGUMENTS is empty - then get copilot review comments from current active git Pull Request
else if $ARGUMENTS is not empty - it might contain something like "PR NN" for a specific PR or just a number like "NN" - then just find that specific one and get the copilot review comments
else if the $ARGUMENTS is something completely different - figure out what it means and handle it creatively

**Procedure**

copilot has made some review comments and now you have to figure out wether this is something we should solve or not. be critical of the review comments - validate/verify them using canonical design sources/documents and existing implementations.

if you are in doubt of something and we need to clarify it together. use the ask tool.