=============
from: e64e0291e854dd5100888e8243ef2f956b02e476
instruction: our codebase depends on `atty` crate which is unmaintained long time, as child dependency of our direct dependency, `clap` and `simple_logger` (see Carge.lock). please version up our direct dependency and remove `atty` dependency from our project.


