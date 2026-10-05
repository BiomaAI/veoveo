UPDATE computer_execution SET next_containment_read=time::now()-1s WHERE containment_reads > 0;
