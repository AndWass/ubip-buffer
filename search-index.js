var searchIndex = JSON.parse('{\
"ubip_buffer":{"doc":"A SPSC BipBuffer with external storage.","i":[[3,"StorageRef","ubip_buffer","",null,null],[3,"UnsafeStorageRef","","",null,null],[3,"BipBuffer","","A SPSC BipBuffer with external storage.",null,null],[3,"BipBufferReader","","The reader handle into a `BipBuffer`",null,null],[3,"BipBufferWriter","","The writer handle into a `BipBuffer`",null,null],[4,"CommitError","","",null,null],[13,"NotEnoughPrepared","","",0,null],[12,"prepared","ubip_buffer::CommitError","",1,null],[4,"PrepareError","ubip_buffer","",null,null],[13,"UncommitedData","","",2,null],[12,"amount","ubip_buffer::PrepareError","",3,null],[13,"NoRoom","ubip_buffer","",2,null],[12,"max_available","ubip_buffer::PrepareError","",4,null],[4,"ConsumeError","ubip_buffer","",null,null],[13,"NoneConsumed","","",5,null],[8,"Storage","","Trait used by BipBuffers to reference storage.",null,null],[16,"ValueType","","",6,null],[10,"slice","","Provides a slice over the specified range.",6,[[["range",3]]]],[10,"mut_slice","","Provides a mutable slice over the specified range.",6,[[["range",3]]]],[10,"len","","The total lenght of the storage.",6,[[]]],[11,"new","","",7,[[]]],[11,"new","","",8,[[]]],[11,"capacity","","",9,[[]]],[11,"new","","Construct a new BipBuffer from an external buffer",9,[[]]],[11,"take_reader_writer","","Take readers and writers from the buffer",9,[[],["option",4]]],[11,"values","","Gets a reference to a committed slice of data.",10,[[]]],[11,"consume","","Consume processed values.",10,[[],[["consumeerror",4],["result",4]]]],[11,"capacity","","Returns the total capacity of the `BipBuffer`",11,[[]]],[11,"prepare","","Try to prepare a set amount of data for commitment.",11,[[],[["result",4],["prepareerror",4]]]],[11,"prepare_trailing","","Prepares and returns any part of the buffer trailing the…",11,[[],[["result",4],["prepareerror",4]]]],[11,"prepare_max","","Prepares and returns the biggest slice of continues data…",11,[[],[["result",4],["prepareerror",4]]]],[11,"commit","","Commits a previsouly prepared part of data.",11,[[],[["commiterror",4],["result",4]]]],[11,"discard","","Discards uncommited but prepared data.",11,[[]]],[11,"from","","",7,[[]]],[11,"borrow","","",7,[[]]],[11,"try_from","","",7,[[],["result",4]]],[11,"into","","",7,[[]]],[11,"try_into","","",7,[[],["result",4]]],[11,"borrow_mut","","",7,[[]]],[11,"type_id","","",7,[[],["typeid",3]]],[11,"from","","",8,[[]]],[11,"borrow","","",8,[[]]],[11,"try_from","","",8,[[],["result",4]]],[11,"into","","",8,[[]]],[11,"try_into","","",8,[[],["result",4]]],[11,"borrow_mut","","",8,[[]]],[11,"type_id","","",8,[[],["typeid",3]]],[11,"from","","",9,[[]]],[11,"borrow","","",9,[[]]],[11,"try_from","","",9,[[],["result",4]]],[11,"into","","",9,[[]]],[11,"try_into","","",9,[[],["result",4]]],[11,"borrow_mut","","",9,[[]]],[11,"type_id","","",9,[[],["typeid",3]]],[11,"from","","",10,[[]]],[11,"borrow","","",10,[[]]],[11,"try_from","","",10,[[],["result",4]]],[11,"into","","",10,[[]]],[11,"try_into","","",10,[[],["result",4]]],[11,"borrow_mut","","",10,[[]]],[11,"type_id","","",10,[[],["typeid",3]]],[11,"from","","",11,[[]]],[11,"borrow","","",11,[[]]],[11,"try_from","","",11,[[],["result",4]]],[11,"into","","",11,[[]]],[11,"try_into","","",11,[[],["result",4]]],[11,"borrow_mut","","",11,[[]]],[11,"type_id","","",11,[[],["typeid",3]]],[11,"from","","",0,[[]]],[11,"borrow","","",0,[[]]],[11,"try_from","","",0,[[],["result",4]]],[11,"into","","",0,[[]]],[11,"try_into","","",0,[[],["result",4]]],[11,"borrow_mut","","",0,[[]]],[11,"type_id","","",0,[[],["typeid",3]]],[11,"from","","",2,[[]]],[11,"borrow","","",2,[[]]],[11,"try_from","","",2,[[],["result",4]]],[11,"into","","",2,[[]]],[11,"try_into","","",2,[[],["result",4]]],[11,"borrow_mut","","",2,[[]]],[11,"type_id","","",2,[[],["typeid",3]]],[11,"from","","",5,[[]]],[11,"borrow","","",5,[[]]],[11,"try_from","","",5,[[],["result",4]]],[11,"into","","",5,[[]]],[11,"try_into","","",5,[[],["result",4]]],[11,"borrow_mut","","",5,[[]]],[11,"type_id","","",5,[[],["typeid",3]]],[11,"slice","","",7,[[["range",3]]]],[11,"mut_slice","","",7,[[["range",3]]]],[11,"len","","",7,[[]]],[11,"slice","","",8,[[["range",3]]]],[11,"mut_slice","","",8,[[["range",3]]]],[11,"len","","",8,[[]]],[11,"fmt","","",0,[[["formatter",3]],["result",6]]],[11,"fmt","","",2,[[["formatter",3]],["result",6]]],[11,"fmt","","",5,[[["formatter",3]],["result",6]]],[11,"eq","","",0,[[["commiterror",4]]]],[11,"ne","","",0,[[["commiterror",4]]]],[11,"eq","","",2,[[["prepareerror",4]]]],[11,"ne","","",2,[[["prepareerror",4]]]],[11,"eq","","",5,[[["consumeerror",4]]]]],"p":[[4,"CommitError"],[13,"NotEnoughPrepared"],[4,"PrepareError"],[13,"UncommitedData"],[13,"NoRoom"],[4,"ConsumeError"],[8,"Storage"],[3,"StorageRef"],[3,"UnsafeStorageRef"],[3,"BipBuffer"],[3,"BipBufferReader"],[3,"BipBufferWriter"]]}\
}');
addSearchOptions(searchIndex);initSearch(searchIndex);