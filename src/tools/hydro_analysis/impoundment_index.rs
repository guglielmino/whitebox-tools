/*
This tool is part of the WhiteboxTools geospatial analysis library.
Authors: Dr. John Lindsay
Created: 28/05/2018
Last Modified: 12/10/2018
License: MIT
*/

use crate::raster::*;
use crate::structures::Array2D;
use crate::tools::*;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};
use std::env;
use std::f64;
use std::io::{Error, ErrorKind};
use std::path;
use std::path::Path;

/// This tool can be used to calculate the impoundment size index (ISI) from a digital elevation model (DEM). 
/// The ISI is a land-surface parameter related to the size of the impoundment that would result from inserting 
/// a dam of a user-specified maximum length (`--damlength`) into each DEM grid cell. In addition to an 
/// output dam-height raster (same name as `--output` file but with an *_dam_height* suffix appended), the tool outputs 
/// a measure of impoundment size (`--out_type`) related to impoundment average depth, total volume, or flooded area.
/// 
/// Please note that this tool performs an extremely complex and computationally intensive flow-accumulation operation.
/// As such, it may take a substantial amount of processing time and may encounter issues (including memory issues) when
/// applied to very large DEMs. It is not necessary to pre-process the input DEM (`--dem`) to remove topographic depressions
/// and flat areas. The internal flow-accumulation operation will not be confounded by the presence of these features.
/// 
/// # Reference
/// Lindsay, JB (2015) Modelling the spatial pattern of potential impoundment size from DEMs. 
/// Online resource: [Whitebox Blog](https://whiteboxgeospatial.wordpress.com/2015/04/29/modelling-the-spatial-pattern-of-potential-impoundment-size-from-dems/)
/// 
/// # See Also
/// `StochasticDepressionAnalysis` 
pub struct ImpoundmentSizeIndex {
    name: String,
    description: String,
    toolbox: String,
    parameters: Vec<ToolParameter>,
    example_usage: String,
}

impl ImpoundmentSizeIndex {
    pub fn new() -> ImpoundmentSizeIndex {
        // public constructor
        let name = "ImpoundmentSizeIndex".to_string();
        let toolbox = "Hydrological Analysis".to_string();
        let description =
            "Calculates the impoundment size resulting from damming a DEM.".to_string();

        let mut parameters = vec![];
        parameters.push(ToolParameter {
            name: "Input DEM File".to_owned(),
            flags: vec!["-i".to_owned(), "--dem".to_owned()],
            description: "Input raster DEM file.".to_owned(),
            parameter_type: ParameterType::ExistingFile(ParameterFileType::Raster),
            default_value: None,
            optional: false,
        });

        parameters.push(ToolParameter {
            name: "Output File".to_owned(),
            flags: vec!["-o".to_owned(), "--output".to_owned()],
            description: "Output file.".to_owned(),
            parameter_type: ParameterType::NewFile(ParameterFileType::Raster),
            default_value: None,
            optional: false,
        });

        parameters.push(ToolParameter {
            name: "Output Type".to_owned(),
            flags: vec!["--out_type".to_owned()],
            description: "Output type; one of 'depth' (default), 'volume', and 'area'.".to_owned(),
            parameter_type: ParameterType::OptionList(vec![
                "depth".to_owned(),
                "volume".to_owned(),
                "area".to_owned(),
            ]),
            default_value: Some("depth".to_owned()),
            optional: true,
        });

        parameters.push(ToolParameter {
            name: "Max dam length (grid cells)".to_owned(),
            flags: vec!["--damlength".to_owned()],
            description: "Maximum length of thr dam.".to_owned(),
            parameter_type: ParameterType::Float,
            default_value: None,
            optional: false,
        });

        let sep: String = path::MAIN_SEPARATOR.to_string();
        let p = format!("{}", env::current_dir().unwrap().display());
        let e = format!("{}", env::current_exe().unwrap().display());
        let mut short_exe = e
            .replace(&p, "")
            .replace(".exe", "")
            .replace(".", "")
            .replace(&sep, "");
        if e.contains(".exe") {
            short_exe += ".exe";
        }
        let usage = format!(">>.*{0} -r={1} -v --wd=\"*path*to*data*\" --dem=DEM.tif -o=out.tif --out_type=depth --damlength=11", short_exe, name).replace("*", &sep);

        ImpoundmentSizeIndex {
            name: name,
            description: description,
            toolbox: toolbox,
            parameters: parameters,
            example_usage: usage,
        }
    }
}

impl WhiteboxTool for ImpoundmentSizeIndex {
    fn get_source_file(&self) -> String {
        String::from(file!())
    }

    fn get_tool_name(&self) -> String {
        self.name.clone()
    }

    fn get_tool_description(&self) -> String {
        self.description.clone()
    }

    fn get_tool_parameters(&self) -> String {
        let mut s = String::from("{\"parameters\": [");
        for i in 0..self.parameters.len() {
            if i < self.parameters.len() - 1 {
                s.push_str(&(self.parameters[i].to_string()));
                s.push_str(",");
            } else {
                s.push_str(&(self.parameters[i].to_string()));
            }
        }
        s.push_str("]}");
        s
    }

    fn get_example_usage(&self) -> String {
        self.example_usage.clone()
    }

    fn get_toolbox(&self) -> String {
        self.toolbox.clone()
    }

    fn run<'a>(
        &self,
        args: Vec<String>,
        working_directory: &'a str,
        verbose: bool,
    ) -> Result<(), Error> {
        let mut input_file = String::new();
        let mut output_file = String::new();
        let mut out_type = 0; // 0 = area; 1 = volume
        let mut dam_length = 111f64;

        if args.len() == 0 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Tool run with no paramters.",
            ));
        }
        for i in 0..args.len() {
            let mut arg = args[i].replace("\"", "");
            arg = arg.replace("\'", "");
            let cmd = arg.split("="); // in case an equals sign was used
            let vec = cmd.collect::<Vec<&str>>();
            let mut keyval = false;
            if vec.len() > 1 {
                keyval = true;
            }
            let flag_val = vec[0].to_lowercase().replace("--", "-");
            if flag_val == "-i" || flag_val == "-dem" {
                input_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-o" || flag_val == "-output" {
                output_file = if keyval {
                    vec[1].to_string()
                } else {
                    args[i + 1].to_string()
                };
            } else if flag_val == "-out_type" {
                let val = if keyval {
                    vec[1].to_lowercase()
                } else {
                    args[i + 1].to_lowercase()
                };
                out_type = if val.contains("v") {
                    1
                } else if val.contains("depth") {
                    2
                } else {
                    0 // area
                };
            } else if flag_val == "-damlength" {
                dam_length = if keyval {
                    vec[1].to_string().parse::<f64>().unwrap()
                } else {
                    args[i + 1].to_string().parse::<f64>().unwrap()
                };
            }
        }

        let mut progress: usize;
        let mut old_progress: usize = 1;

        if verbose {
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
            println!("* Welcome to {} *", self.get_tool_name());
            println!("***************{}", "*".repeat(self.get_tool_name().len()));
        }

        let sep: String = path::MAIN_SEPARATOR.to_string();

        if !input_file.contains(&sep) && !input_file.contains("/") {
            input_file = format!("{}{}", working_directory, input_file);
        }
        if !output_file.contains(&sep) && !output_file.contains("/") {
            output_file = format!("{}{}", working_directory, output_file);
        }

        /*
        There are three stages to the calculation of the impoundment index:

        1. Calculate the dam height. This involves examining the various
        topographic profiles centred on each grid cell in the DEM (oriented
        in each of the cardinal directions) and determining the largest
        dam feature, of a user-specified length, that can be built through
        the point.

        2. A priority flood operation is performed to calculate flow directions,
        the number of inflowing neighbours, and the maximum downstream dam elevation.

        3. A flow-path tracing operation is used for the flow accumulation. This
        operation calculates the number of upslope grid cells that are less
        than the calculated dam height.
        */

        if verbose {
            println!("Reading data...")
        };

        // let input = Arc::new(Raster::new(&input_file, "r")?);
        let input = Raster::new(&input_file, "r")?;

        let start = Instant::now();
        let rows = input.configs.rows as isize;
        let columns = input.configs.columns as isize;
        let num_cells = rows * columns;
        let nodata = input.configs.nodata;
        let grid_area = input.configs.resolution_x * input.configs.resolution_y;

        // Calculate dam heights
        /*
        Each cell will be assigned the altitude (ASL) of the highest dam that
        passes through the cell. Potential dams are calculated for each
        grid cell in the N-S, NE-SW, E-W, SE-NW directions.

        The dam heights are used to calculate the 'threshold' in the index
        calculation. The threshold elevation is the elevation below which any
        upstream cells are considered part of the impoundment created by placing
        a dam through the associated grid cell.
        */
        let mut crest_elev: Array2D<f64> = Array2D::new(rows, columns, -32768f64, nodata)?;
        let dx = [1, 1, 1, 0, -1, -1, -1, 0];
        let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
        // The following perpendicular direction represent perpendiculars
        // to the NE-SW, E-W, SE-NW, and N-S directions.
        let perpendicular1 = [2, 3, 4, 1];
        let perpendicular2 = [6, 7, 0, 5];
        let half_dam_length = (dam_length / 2f64).floor() as usize;
        let dam_profile_length = half_dam_length * 2 + 1;
        let mut dam_profile = vec![0f64; dam_profile_length];
        let mut dam_profile_filled = vec![0f64; dam_profile_length];
        let (mut perp_dir1, mut perp_dir2): (i8, i8);
        let mut z: f64;
        let mut z_n: f64;
        let (mut r_n, mut c_n, mut r_n2, mut c_n2): (isize, isize, isize, isize);
        for row in 0..rows {
            for col in 0..columns {
                z = input.get_value(row, col);
                if z != nodata {
                    for dir in 0..4 {
                        // what's the perpendicular direction?
                        perp_dir1 = perpendicular1[dir];
                        perp_dir2 = perpendicular2[dir];
                        dam_profile[half_dam_length] = input.get_value(row, col);

                        // find the profile elevations
                        r_n = row;
                        c_n = col;
                        r_n2 = row;
                        c_n2 = col;
                        for i in 1..=half_dam_length {
                            r_n += dy[perp_dir1 as usize];
                            c_n += dx[perp_dir1 as usize];
                            z_n = input.get_value(r_n, c_n);
                            if z_n != nodata {
                                dam_profile[half_dam_length + i as usize] = z_n;
                            } else {
                                dam_profile[half_dam_length + i as usize] = f64::NEG_INFINITY;
                            }

                            r_n2 += dy[perp_dir2 as usize];
                            c_n2 += dx[perp_dir2 as usize];
                            z_n = input.get_value(r_n2, c_n2);
                            if z_n != nodata {
                                dam_profile[half_dam_length - i] = z_n;
                            } else {
                                dam_profile[half_dam_length - i] = f64::NEG_INFINITY;
                            }
                        }

                        dam_profile_filled[0] = dam_profile[0];
                        for i in 1..dam_profile_length - 1 {
                            if dam_profile_filled[i - 1] > dam_profile[i] {
                                dam_profile_filled[i] = dam_profile_filled[i - 1];
                            } else {
                                dam_profile_filled[i] = dam_profile[i];
                            }
                        }

                        dam_profile_filled[dam_profile_length - 1] =
                            dam_profile[dam_profile_length - 1];
                        for i in (1..dam_profile_length - 1).rev() {
                            if dam_profile_filled[i + 1] > dam_profile[i] {
                                if dam_profile_filled[i + 1] < dam_profile_filled[i] {
                                    dam_profile_filled[i] = dam_profile_filled[i + 1];
                                }
                            } else {
                                dam_profile_filled[i] = dam_profile[i];
                            }
                        }

                        if dam_profile_filled[half_dam_length] > crest_elev.get_value(row, col) {
                            crest_elev.set_value(row, col, dam_profile_filled[half_dam_length]);
                        }
                        r_n = row;
                        c_n = col;
                        r_n2 = row;
                        c_n2 = col;
                        for i in 1..=half_dam_length {
                            r_n += dy[perp_dir1 as usize];
                            c_n += dx[perp_dir1 as usize];
                            z_n = input.get_value(r_n, c_n);
                            if z_n != nodata {
                                if dam_profile_filled[half_dam_length + i as usize]
                                    > crest_elev.get_value(r_n, c_n)
                                {
                                    crest_elev.set_value(
                                        r_n,
                                        c_n,
                                        dam_profile_filled[half_dam_length + i as usize],
                                    );
                                }
                            }

                            r_n2 += dy[perp_dir2 as usize];
                            c_n2 += dx[perp_dir2 as usize];
                            z_n = input.get_value(r_n2, c_n2);
                            if z_n != nodata {
                                if dam_profile_filled[half_dam_length - i as usize]
                                    > crest_elev.get_value(r_n2, c_n2)
                                {
                                    crest_elev.set_value(
                                        r_n2,
                                        c_n2,
                                        dam_profile_filled[half_dam_length - i as usize],
                                    );
                                }
                            }
                        }
                    }
                } else {
                    crest_elev.set_value(row, col, nodata);
                }
            }
            if verbose {
                progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Calculating dam heights: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        /*
        The following steps are part of a priority flood operation. This operation serves
        several purposes. First, it is used to calculate the flow directions and number
        of inflowing neighbourings for each grid cell. These are useful during the index
        calculation, which is essentially a flow accumulation operation that progresses as
        a flow-path tracing operation from the divide cells downstream. Secondly, the priority
        flood operation is useful for calculating the maximum downstream dam height, stored in
        the filled_dem Array2D. These data serve as the 'cutoff_z' variable in the calculation
        of the index. Elevation values contained within the accumulated elevation list that are
        less than the cuttoff_z for a grid cell are propagated to the next downstream cell.
        That is, the dam elevation at a cell determines which upslope cells are within the
        flooded area, and the maximum downstream dam elevation determines which upstream cells
        are accumulated downstream. Because a downstream dam elevation may actually be higher
        than an upstream cell's dam elevation, there is a need to keep track of both of these
        threshold elevation values.
        */
        let background_val = (i32::min_value() + 1) as f64;
        let mut filled_dem: Array2D<f64> = Array2D::new(rows, columns, background_val, nodata)?;
        let mut flow_dir: Array2D<i8> = Array2D::new(rows, columns, -1, -1)?;

        /*
        Find the data edges. This is complicated by the fact that DEMs frequently
        have nodata edges, whereby the DEM does not occupy the full extent of
        the raster. One approach to doing this would be simply to scan the
        raster, looking for cells that neighbour nodata values. However, this
        assumes that there are no interior nodata holes in the dataset. Instead,
        the approach used here is to perform a region-growing operation, looking
        for nodata values along the raster's edges.
        */

        let mut queue: VecDeque<(isize, isize)> =
            VecDeque::with_capacity((rows * columns) as usize);
        for row in 0..rows {
            /*
            Note that this is only possible because Whitebox rasters
            allow you to address cells beyond the raster extent but
            return the nodata value for these regions.
            */
            queue.push_back((row, -1));
            queue.push_back((row, columns));
        }

        for col in 0..columns {
            queue.push_back((-1, col));
            queue.push_back((rows, col));
        }

        /*
        minheap is the priority queue. Note that I've tested using integer-based
        priority values, by multiplying the elevations, but this didn't result
        in a significant performance gain over the use of f64s.
        */
        let mut minheap = BinaryHeap::with_capacity((rows * columns) as usize);
        let mut num_solved_cells = 0;
        let mut zin_n: f64; // value of neighbour of row, col in input raster
        let mut zout: f64; // value of row, col in output raster
        let mut zout_n: f64; // value of neighbour of row, col in output raster
                             // let dx = [1, 1, 1, 0, -1, -1, -1, 0];
                             // let dy = [-1, 0, 1, 1, 1, 0, -1, -1];
        let (mut row, mut col): (isize, isize);
        let (mut row_n, mut col_n): (isize, isize);
        let mut num_nodata_cells = 0;
        while !queue.is_empty() {
            let cell = queue.pop_front().unwrap();
            row = cell.0;
            col = cell.1;
            for n in 0..8 {
                row_n = row + dy[n];
                col_n = col + dx[n];
                zin_n = input.get_value(row_n, col_n);
                zout_n = filled_dem.get_value(row_n, col_n);
                if zout_n == background_val {
                    if zin_n == nodata {
                        filled_dem.set_value(row_n, col_n, nodata);
                        queue.push_back((row_n, col_n));
                        num_nodata_cells += 1;
                    } else {
                        // filled_dem.set_value(row_n, col_n, zin_n);
                        filled_dem.set_value(row_n, col_n, crest_elev.get_value(row_n, col_n));
                        // Push it onto the priority queue for the priority flood operation
                        minheap.push(GridCell {
                            row: row_n,
                            column: col_n,
                            priority: zin_n,
                        });
                    }
                    num_solved_cells += 1;
                }
            }

            if verbose {
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Calculating flow directions: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // Perform the priority flood operation.
        // Also calculate the number of inflowing cells
        let back_link = [4i8, 5i8, 6i8, 7i8, 0i8, 1i8, 2i8, 3i8];
        let mut dir: i8;
        let mut count: i8;
        let mut num_inflowing: Array2D<i8> = Array2D::new(rows, columns, -1, -1)?;
        let mut stack = Vec::with_capacity((rows * columns) as usize);
        while !minheap.is_empty() {
            let cell = minheap.pop().unwrap();
            row = cell.row;
            col = cell.column;
            zout = filled_dem.get_value(row, col);
            count = 0;
            for n in 0..8 {
                row_n = row + dy[n];
                col_n = col + dx[n];
                zout_n = filled_dem.get_value(row_n, col_n);
                if zout_n == background_val {
                    zin_n = crest_elev.get_value(row_n, col_n);
                    if zin_n != nodata {
                        flow_dir.set_value(row_n, col_n, back_link[n]);
                        count += 1;
                        if zin_n < zout {
                            zin_n = zout;
                        }
                        filled_dem.set_value(row_n, col_n, zin_n);
                        minheap.push(GridCell {
                            row: row_n,
                            column: col_n,
                            priority: input.get_value(row_n, col_n),
                        });
                    } else {
                        // Interior nodata cells are still treated as nodata and are not filled.
                        filled_dem.set_value(row_n, col_n, nodata);
                        num_nodata_cells += 1;
                    }
                }
            }

            num_inflowing.set_value(row, col, count);
            if count == 0i8 {
                stack.push((row, col));
            }

            if verbose {
                num_solved_cells += 1;
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Calculating flow directions: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        /*
        Perform the index calculation. This is essentially a downstream-directed flow-path
        tracing and accumulation operation that begins at the divides and ends at outlets.
        Divides are cells with no inflowing neighbours. A flow-path continues downstream
        only once each inflowing cell has been solved. The flow-accumulation component
        propagates elevation values from upstream to downstream. Only those upstream
        elevations that are less than the maximum downstream dam elevation are propagated
        downstream. In upslope and divergent areas, this will be very few cell elevaitons.
        In deeply incised downstream areas, this may be many cells. For each grid cell,
        the index will count the number of upstream cells that have a lower elevation than
        the cell's calculated dam elevation. This can be reported either as a reservoir volume
        or an area.
        */
        let mut upslope_elevs: Vec<Vec<Vec<f64>>> =
            vec![vec![vec![]; columns as usize]; rows as usize];
        num_solved_cells = num_nodata_cells;
        let mut z: f64;
        let mut cutoff_z: f64;
        let mut threshold: f64;
        let mut num_upslope: f64;
        let mut vol: f64;
        let mut output = Raster::initialize_using_file(&output_file, &input);
        output.reinitialize_values(0.0);
        while !stack.is_empty() {
            let cell = stack.pop().unwrap();
            row = cell.0;
            col = cell.1;
            z = input.get_value(row, col);
            num_inflowing.decrement(row, col, 1i8);
            dir = flow_dir.get_value(row, col);
            if dir >= 0 {
                row_n = row + dy[dir as usize];
                col_n = col + dx[dir as usize];
                // Pass the upslope elevations that are lower than
                // the cutoff elevation downslope
                cutoff_z = filled_dem.get_value(row_n, col_n);
                threshold = crest_elev.get_value(row_n, col_n);
                num_upslope = 0f64;
                vol = 0f64;
                upslope_elevs[row as usize][col as usize].push(z); // adding the elevation of row, col
                for up_z in upslope_elevs[row as usize][col as usize].clone() {
                    if up_z < cutoff_z {
                        upslope_elevs[row_n as usize][col_n as usize].push(up_z);
                        if up_z < threshold {
                            num_upslope += 1f64;
                            vol += threshold - up_z;
                        }
                    }
                }
                upslope_elevs[row as usize][col as usize] = vec![];

                if out_type == 0 {
                    // area
                    output.increment(row_n, col_n, num_upslope * grid_area);
                } else if out_type == 1 {
                    // volume
                    output.increment(row_n, col_n, vol);
                } else {
                    // mean depth
                    if num_upslope > 0f64 {
                        output.increment(row_n, col_n, vol / (num_upslope * grid_area));
                    }
                }

                num_inflowing.decrement(row_n, col_n, 1i8);
                if num_inflowing[(row_n, col_n)] == 0i8 {
                    stack.push((row_n, col_n));
                }
            }

            if verbose {
                num_solved_cells += 1;
                progress = (100.0_f64 * num_solved_cells as f64 / (num_cells - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Calculating index: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        // Output the dam height above the ground elevation
        let extension: String = match Path::new(&output_file).extension().unwrap().to_str() {
            Some(n) => n.to_string(),
            None => "".to_string(),
        };
        let output_hgt_file = &output_file.replace(
            &format!(".{}", extension),
            &format!("_dam_height.{}", extension),
        );
        let mut output_hgt = Raster::initialize_using_file(&output_hgt_file, &input);
        for row in 0..rows {
            for col in 0..columns {
                z = input.get_value(row, col);
                if z != nodata {
                    output_hgt.set_value(row, col, crest_elev.get_value(row, col) - z);
                } else {

                }
            }
            if verbose {
                progress = (100.0_f64 * row as f64 / (rows - 1) as f64) as usize;
                if progress != old_progress {
                    println!("Outputting dam heights: {}%", progress);
                    old_progress = progress;
                }
            }
        }

        let elapsed_time = get_formatted_elapsed_time(start);

        output.configs.palette = "spectrum.plt".to_string();
        output.add_metadata_entry(format!(
            "Created by whitebox_tools\' {} tool",
            self.get_tool_name()
        ));
        output.add_metadata_entry(format!("Input file: {}", input_file));
        output.add_metadata_entry(format!("Dam length: {}", dam_length));
        if out_type == 0 {
            output.add_metadata_entry(format!("Out type: flooded area"));
        } else if out_type == 1 {
            output.add_metadata_entry(format!("Out type: reservoir volume"));
        } else {
            output.add_metadata_entry(format!("Out type: average reservoir depth"));
        }
        output.add_metadata_entry(format!("Elapsed Time (excluding I/O): {}", elapsed_time));

        if verbose {
            println!("Saving index data...")
        };
        let _ = match output.write() {
            Ok(_) => {
                if verbose {
                    println!("Output file written")
                }
            }
            Err(e) => return Err(e),
        };

        output_hgt.configs.palette = "spectrum.plt".to_string();
        output_hgt.add_metadata_entry(format!(
            "Created by whitebox_tools\' {} tool",
            self.get_tool_name()
        ));
        output_hgt.add_metadata_entry(format!("Input file: {}", input_file));
        output_hgt.add_metadata_entry(format!("Dam length: {}", dam_length));
        if out_type == 0 {
            output_hgt.add_metadata_entry(format!("Out type: flooded area"));
        } else {
            output_hgt.add_metadata_entry(format!("Out type: reservoir volume"));
        }
        output_hgt.add_metadata_entry(format!("Elapsed Time (excluding I/O): {}", elapsed_time));

        if verbose {
            println!("Saving dam height data...")
        };
        let _ = match output_hgt.write() {
            Ok(_) => {
                if verbose {
                    println!("Output file written")
                }
            }
            Err(e) => return Err(e),
        };

        if verbose {
            println!(
                "{}",
                &format!("Elapsed Time (excluding I/O): {}", elapsed_time)
            );
        }

        Ok(())
    }
}

#[derive(PartialEq, Debug)]
struct GridCell {
    row: isize,
    column: isize,
    priority: f64,
}

impl Eq for GridCell {}

impl PartialOrd for GridCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        return other.priority.partial_cmp(&self.priority);
    }
}

impl Ord for GridCell {
    fn cmp(&self, other: &GridCell) -> Ordering {
        let ord = self.partial_cmp(other).unwrap();
        match ord {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => ord,
        }
    }
}
