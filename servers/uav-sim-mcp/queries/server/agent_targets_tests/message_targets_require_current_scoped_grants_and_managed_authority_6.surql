UPDATE ONLY $id SET valid_until=NONE, valid_from=time::now()+1h;
