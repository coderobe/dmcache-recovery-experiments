# cache-recovery-experiments

## cache_guess

### USAGE:
    cache_guess [SUBCOMMAND]

### FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

### SUBCOMMANDS:
    collect    
    find       
    help       Prints this message or the help of the given subcommand(s)

## cache_guess collect 
### USAGE:
    cache_guess collect <index> <device>

### FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

### ARGS:
    <index>
    <device>

## cache_guess find 
### USAGE:
    cache_guess find [OPTIONS] <index> <cache_device>

### FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

### OPTIONS:
    --cache-block-size <cache-block-size>    In sectors (512 bytes) [default: 512]

### ARGS:
    <index>
    <cache_device>
